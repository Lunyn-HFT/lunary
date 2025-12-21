use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(feature = "pinning")]
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::Result;
use crate::messages::Message;
use crate::parser::Parser;

pub trait ConcurrentParser: Send {
    fn submit(&self, data: Vec<u8>) -> Result<()>;
    fn recv(&self) -> Option<Vec<Message>>;
    fn try_recv(&self) -> Option<Vec<Message>>;
    fn pending(&self) -> usize;
    fn messages_parsed(&self) -> u64;
    fn bytes_processed(&self) -> u64;
}

pub trait ParserMetrics {
    fn stats_snapshot(&self) -> ParserStatsSnapshot;
    fn reset_stats(&self);
}

pub trait UnifiedParser: Send + Sync {
    fn name(&self) -> &'static str;
    fn submit_data(&self, data: Vec<u8>) -> Result<()>;
    fn receive_messages(&self) -> Option<Vec<Message>>;
    fn try_receive(&self) -> Option<Vec<Message>>;
    fn pending_count(&self) -> usize;
    fn total_messages(&self) -> u64;
    fn total_bytes(&self) -> u64;
    fn total_errors(&self) -> u64;
    fn worker_count(&self) -> usize;
    fn worker_snapshots(&self) -> Vec<WorkerStatsSnapshot>;
    fn throughput_mps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            self.total_messages() as f64 / elapsed_secs
        } else {
            0.0
        }
    }
    fn throughput_mbps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            (self.total_bytes() as f64 / (1024.0 * 1024.0)) / elapsed_secs
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ParserStatsSnapshot {
    pub messages: u64,
    pub bytes: u64,
    pub errors: u64,
    pub batches: u64,
    pub elapsed: Duration,
}

impl ParserStatsSnapshot {
    #[inline]
    pub fn throughput_mps(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
            self.messages as f64 / secs
        } else {
            0.0
        }
    }

    #[inline]
    pub fn throughput_mbps(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
            (self.bytes as f64 / (1024.0 * 1024.0)) / secs
        } else {
            0.0
        }
    }

    #[inline]
    pub fn avg_batch_size(&self) -> f64 {
        if self.batches > 0 {
            self.messages as f64 / self.batches as f64
        } else {
            0.0
        }
    }
}

#[cfg(feature = "pinning")]
static CORE_POOL: OnceLock<Vec<core_affinity::CoreId>> = OnceLock::new();
#[cfg(feature = "pinning")]
static PINNING_ENABLED: AtomicBool = AtomicBool::new(true);
#[cfg(feature = "pinning")]
static PIN_ENV_DISABLE: OnceLock<bool> = OnceLock::new();
#[cfg(feature = "pinning")]
static NEXT_CORE: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "pinning")]
#[inline]
fn pin_current_thread() {
    let disable_env =
        *PIN_ENV_DISABLE.get_or_init(|| std::env::var("LUNARY_DISABLE_PINNING").is_ok());
    if disable_env || !PINNING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let cores = CORE_POOL.get_or_init(|| core_affinity::get_core_ids().unwrap_or_default());
    if cores.is_empty() {
        PINNING_ENABLED.store(false, Ordering::Relaxed);
        return;
    }

    let idx = NEXT_CORE.fetch_add(1, Ordering::Relaxed);
    let target = cores[idx % cores.len()];
    if !core_affinity::set_for_current(target) {
        PINNING_ENABLED.store(false, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "pinning"))]
#[inline]
fn pin_current_thread() {}

const SPSC_BUFFER_SIZE: usize = 4096;
const CACHE_LINE_SIZE: usize = 64;

pub struct SpscQueue<T> {
    buffer: Box<[std::cell::UnsafeCell<Option<T>>; SPSC_BUFFER_SIZE]>,
    _pad1: [u8; CACHE_LINE_SIZE],
    head: AtomicUsize,
    _pad2: [u8; CACHE_LINE_SIZE - std::mem::size_of::<AtomicUsize>()],
    tail: AtomicUsize,
    _pad3: [u8; CACHE_LINE_SIZE - std::mem::size_of::<AtomicUsize>()],
    cached_head: std::cell::Cell<usize>,
    cached_tail: std::cell::Cell<usize>,
}

unsafe impl<T: Send> Send for SpscQueue<T> {}
unsafe impl<T: Send> Sync for SpscQueue<T> {}

impl<T> SpscQueue<T> {
    pub fn new() -> Self {
        let buffer: Box<[std::cell::UnsafeCell<Option<T>>; SPSC_BUFFER_SIZE]> = {
            let mut v = Vec::with_capacity(SPSC_BUFFER_SIZE);
            for _ in 0..SPSC_BUFFER_SIZE {
                v.push(std::cell::UnsafeCell::new(None));
            }
            v.try_into()
                .expect("Vec length matches SPSC_BUFFER_SIZE; conversion cannot fail")
        };
        Self {
            buffer,
            _pad1: [0; CACHE_LINE_SIZE],
            head: AtomicUsize::new(0),
            _pad2: [0; CACHE_LINE_SIZE - std::mem::size_of::<AtomicUsize>()],
            tail: AtomicUsize::new(0),
            _pad3: [0; CACHE_LINE_SIZE - std::mem::size_of::<AtomicUsize>()],
            cached_head: std::cell::Cell::new(0),
            cached_tail: std::cell::Cell::new(0),
        }
    }

    #[inline(always)]
    pub fn push(&self, value: T) -> std::result::Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % SPSC_BUFFER_SIZE;

        let cached = self.cached_head.get();
        if next_tail == cached {
            let head = self.head.load(Ordering::Acquire);
            self.cached_head.set(head);
            if next_tail == head {
                return Err(value);
            }
        }

        unsafe {
            *self.buffer[tail].get() = Some(value);
        }
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }

    #[inline(always)]
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);

        let cached = self.cached_tail.get();
        if head == cached {
            let tail = self.tail.load(Ordering::Acquire);
            self.cached_tail.set(tail);
            if head == tail {
                return None;
            }
        }

        let value = unsafe { (*self.buffer[head].get()).take() };
        self.head
            .store((head + 1) % SPSC_BUFFER_SIZE, Ordering::Release);
        value
    }

    #[inline]
    pub fn pop_batch(&self, batch: &mut Vec<T>, max_count: usize) -> usize {
        let mut count = 0;
        while count < max_count {
            match self.pop() {
                Some(v) => {
                    batch.push(v);
                    count += 1;
                }
                None => break,
            }
        }
        count
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Relaxed) == self.tail.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);
        if tail >= head {
            tail - head
        } else {
            SPSC_BUFFER_SIZE - head + tail
        }
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        SPSC_BUFFER_SIZE - 1
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % SPSC_BUFFER_SIZE;
        next_tail == self.head.load(Ordering::Acquire)
    }
}

impl<T> Default for SpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SpscParser {
    input_queue: Arc<SpscQueue<WorkUnit>>,
    output_queue: Arc<SpscQueue<Vec<Message>>>,
    worker: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    stats: Arc<AtomicStats>,
    start_time: Instant,
}

impl SpscParser {
    pub fn new() -> Self {
        let input_queue: Arc<SpscQueue<WorkUnit>> = Arc::new(SpscQueue::new());
        let output_queue: Arc<SpscQueue<Vec<Message>>> = Arc::new(SpscQueue::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(AtomicStats::new());

        let input_q = Arc::clone(&input_queue);
        let output_q = Arc::clone(&output_queue);
        let shutdown_flag = Arc::clone(&shutdown);
        let stats_ref = Arc::clone(&stats);

        let worker = thread::spawn(move || {
            pin_current_thread();
            let mut parser = Parser::new();

            loop {
                if shutdown_flag.load(Ordering::Relaxed) {
                    break;
                }

                match input_q.pop() {
                    Some(work_unit) => {
                        let (data_slice, data_len) = match &work_unit {
                            WorkUnit::Owned(v) => (v.as_slice(), v.len()),
                            WorkUnit::ArcSlice(arc, start, end) => {
                                (&arc[*start..*end], end - start)
                            }
                        };
                        match parser.parse_all(data_slice) {
                            Ok(messages) => {
                                stats_ref.add_messages(messages.len() as u64);
                                stats_ref.add_bytes(data_len as u64);
                                let _ = output_q.push(messages);
                            }
                            Err(_) => {
                                stats_ref.add_error();
                                let _ = output_q.push(Vec::new());
                            }
                        }
                        parser.reset();
                    }
                    None => {
                        std::hint::spin_loop();
                    }
                }
            }
        });

        Self {
            input_queue,
            output_queue,
            worker: Some(worker),
            shutdown,
            stats,
            start_time: Instant::now(),
        }
    }

    pub fn submit_arc(&self, data: Arc<[u8]>, start: usize, end: usize) -> Result<()> {
        if start >= end || end > data.len() {
            return Err(crate::error::ParseError::InvalidArgument(format!(
                "invalid range {}..{} (len={})",
                start,
                end,
                data.len()
            )));
        }
        self.input_queue
            .push(WorkUnit::ArcSlice(data, start, end))
            .map_err(|d| {
                let size = match &d {
                    WorkUnit::Owned(v) => v.len(),
                    WorkUnit::ArcSlice(_, s, e) => e - s,
                };
                crate::error::ParseError::BufferOverflow {
                    size,
                    max: SPSC_BUFFER_SIZE,
                }
            })
    }
}

impl ConcurrentParser for SpscParser {
    fn submit(&self, data: Vec<u8>) -> Result<()> {
        self.input_queue.push(WorkUnit::Owned(data)).map_err(|d| {
            let size = match &d {
                WorkUnit::Owned(v) => v.len(),
                WorkUnit::ArcSlice(_, s, e) => e - s,
            };
            crate::error::ParseError::BufferOverflow {
                size,
                max: SPSC_BUFFER_SIZE,
            }
        })
    }

    fn recv(&self) -> Option<Vec<Message>> {
        loop {
            if let Some(msgs) = self.output_queue.pop() {
                return Some(msgs);
            }
            if self.shutdown.load(Ordering::Relaxed) {
                return None;
            }
            std::hint::spin_loop();
        }
    }

    fn try_recv(&self) -> Option<Vec<Message>> {
        self.output_queue.pop()
    }

    fn pending(&self) -> usize {
        self.input_queue.len()
    }

    fn messages_parsed(&self) -> u64 {
        self.stats.messages()
    }

    fn bytes_processed(&self) -> u64 {
        self.stats.bytes()
    }
}

impl ParserMetrics for SpscParser {
    fn stats_snapshot(&self) -> ParserStatsSnapshot {
        ParserStatsSnapshot {
            messages: self.stats.messages(),
            bytes: self.stats.bytes(),
            errors: self.stats.errors(),
            batches: 0,
            elapsed: self.start_time.elapsed(),
        }
    }

    fn reset_stats(&self) {
        self.stats.reset();
    }
}

impl Default for SpscParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SpscParser {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct ParallelParser {
    workers: Vec<JoinHandle<()>>,
    sender: Sender<WorkUnit>,
    result_receiver: Receiver<Vec<Message>>,
    shutdown: Arc<AtomicBool>,
    messages_parsed: Arc<AtomicU64>,
    bytes_processed: Arc<AtomicU64>,
    worker_stats: Arc<Vec<WorkerStats>>,
    errors: Arc<AtomicU64>,
    start_time: Instant,
}

enum WorkUnit {
    Owned(Vec<u8>),
    ArcSlice(Arc<[u8]>, usize, usize),
}

impl ParallelParser {
    pub fn new(num_workers: usize) -> Self {
        let num_workers = num_workers.max(1);
        let (work_sender, work_receiver) = bounded::<WorkUnit>(num_workers * 2);
        let (result_sender, result_receiver) = bounded::<Vec<Message>>(num_workers * 2);
        let shutdown = Arc::new(AtomicBool::new(false));
        let messages_parsed = Arc::new(AtomicU64::new(0));
        let bytes_processed = Arc::new(AtomicU64::new(0));
        let errors = Arc::new(AtomicU64::new(0));

        let worker_stats: Arc<Vec<WorkerStats>> =
            Arc::new((0..num_workers).map(WorkerStats::new).collect());

        let mut workers = Vec::with_capacity(num_workers);

        for worker_id in 0..num_workers {
            let rx = work_receiver.clone();
            let tx = result_sender.clone();
            let shutdown_flag = Arc::clone(&shutdown);
            let msg_counter = Arc::clone(&messages_parsed);
            let byte_counter = Arc::clone(&bytes_processed);
            let err_counter = Arc::clone(&errors);
            let stats = Arc::clone(&worker_stats);

            let handle = thread::spawn(move || {
                pin_current_thread();
                let mut parser = Parser::new();

                while !shutdown_flag.load(Ordering::Relaxed) {
                    match rx.recv() {
                        Ok(work) => {
                            match work {
                                WorkUnit::Owned(data_vec) => {
                                    let data_len = data_vec.len();
                                    match parser.parse_all(&data_vec) {
                                        Ok(messages) => {
                                            let msg_count = messages.len() as u64;
                                            msg_counter.fetch_add(msg_count, Ordering::Relaxed);
                                            byte_counter
                                                .fetch_add(data_len as u64, Ordering::Relaxed);
                                            stats[worker_id]
                                                .record_batch(msg_count, data_len as u64);
                                            let _ = tx.send(messages);
                                        }
                                        Err(_) => {
                                            err_counter.fetch_add(1, Ordering::Relaxed);
                                            stats[worker_id].record_error();
                                            let _ = tx.send(Vec::new());
                                        }
                                    }
                                }
                                WorkUnit::ArcSlice(arc, start, end) => {
                                    let slice = &arc[start..end];
                                    let data_len = slice.len();
                                    match parser.parse_all(slice) {
                                        Ok(messages) => {
                                            let msg_count = messages.len() as u64;
                                            msg_counter.fetch_add(msg_count, Ordering::Relaxed);
                                            byte_counter
                                                .fetch_add(data_len as u64, Ordering::Relaxed);
                                            stats[worker_id]
                                                .record_batch(msg_count, data_len as u64);
                                            let _ = tx.send(messages);
                                        }
                                        Err(_) => {
                                            err_counter.fetch_add(1, Ordering::Relaxed);
                                            stats[worker_id].record_error();
                                            let _ = tx.send(Vec::new());
                                        }
                                    }
                                }
                            }
                            parser.reset();
                        }
                        Err(_) => break,
                    }
                }
            });

            workers.push(handle);
        }

        Self {
            workers,
            sender: work_sender,
            result_receiver,
            shutdown,
            messages_parsed,
            bytes_processed,
            worker_stats,
            errors,
            start_time: Instant::now(),
        }
    }

    pub fn submit(&self, data: Vec<u8>) -> Result<()> {
        self.sender.send(WorkUnit::Owned(data)).map_err(|e| {
            let size = match &e.0 {
                WorkUnit::Owned(v) => v.len(),
                WorkUnit::ArcSlice(_, s, e) => e - s,
            };
            crate::error::ParseError::BufferOverflow { size, max: 0 }
        })
    }

    pub fn submit_arc(&self, data: Arc<[u8]>, start: usize, end: usize) -> Result<()> {
        if start >= end || end > data.len() {
            return Err(crate::error::ParseError::InvalidArgument(format!(
                "invalid range {}..{} (len={})",
                start,
                end,
                data.len()
            )));
        }
        self.sender
            .send(WorkUnit::ArcSlice(data, start, end))
            .map_err(|e| {
                let size = match &e.0 {
                    WorkUnit::Owned(v) => v.len(),
                    WorkUnit::ArcSlice(_, s, e) => e - s,
                };
                crate::error::ParseError::BufferOverflow { size, max: 0 }
            })
    }

    pub fn submit_chunk(&self, data: Arc<[u8]>, chunk_size: usize) -> Result<usize> {
        let mut submitted = 0;
        let mut offset = 0;
        while offset < data.len() {
            let end = std::cmp::min(offset + chunk_size, data.len());
            self.submit_arc(Arc::clone(&data), offset, end)?;
            submitted += 1;
            offset = end;
        }
        Ok(submitted)
    }

    pub fn recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.recv().ok()
    }

    pub fn try_recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.try_recv().ok()
    }

    pub fn pending(&self) -> usize {
        self.sender.len()
    }

    pub fn results_ready(&self) -> usize {
        self.result_receiver.len()
    }

    pub fn messages_parsed(&self) -> u64 {
        self.messages_parsed.load(Ordering::Relaxed)
    }

    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed.load(Ordering::Relaxed)
    }

    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn worker_stats(&self) -> Vec<WorkerStatsSnapshot> {
        self.worker_stats.iter().map(|s| s.snapshot()).collect()
    }

    pub fn num_workers(&self) -> usize {
        self.worker_stats.len()
    }

    pub fn shutdown(self) {
        self.shutdown.store(true, Ordering::Relaxed);
        drop(self.sender);
        for worker in self.workers {
            let _ = worker.join();
        }
    }
}

impl Default for ParallelParser {
    fn default() -> Self {
        Self::new(num_cpus())
    }
}

impl ParserMetrics for ParallelParser {
    fn stats_snapshot(&self) -> ParserStatsSnapshot {
        ParserStatsSnapshot {
            messages: self.messages_parsed.load(Ordering::Relaxed),
            bytes: self.bytes_processed.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            batches: self
                .worker_stats
                .iter()
                .map(|s| s.batches.load(Ordering::Relaxed))
                .sum(),
            elapsed: self.start_time.elapsed(),
        }
    }

    fn reset_stats(&self) {
        self.messages_parsed.store(0, Ordering::Relaxed);
        self.bytes_processed.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        for ws in self.worker_stats.iter() {
            ws.reset();
        }
    }
}

impl ConcurrentParser for ParallelParser {
    fn submit(&self, data: Vec<u8>) -> Result<()> {
        let size = data.len();
        self.sender
            .send(WorkUnit::Owned(data))
            .map_err(|_| crate::error::ParseError::BufferOverflow { size, max: 0 })
    }

    fn recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.recv().ok()
    }

    fn try_recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.try_recv().ok()
    }

    fn pending(&self) -> usize {
        self.sender.len()
    }

    fn messages_parsed(&self) -> u64 {
        self.messages_parsed.load(Ordering::Relaxed)
    }

    fn bytes_processed(&self) -> u64 {
        self.bytes_processed.load(Ordering::Relaxed)
    }
}

impl UnifiedParser for ParallelParser {
    fn name(&self) -> &'static str {
        "ParallelParser"
    }
    fn submit_data(&self, data: Vec<u8>) -> Result<()> {
        ConcurrentParser::submit(self, data)
    }
    fn receive_messages(&self) -> Option<Vec<Message>> {
        ConcurrentParser::recv(self)
    }
    fn try_receive(&self) -> Option<Vec<Message>> {
        ConcurrentParser::try_recv(self)
    }
    fn pending_count(&self) -> usize {
        ConcurrentParser::pending(self)
    }
    fn total_messages(&self) -> u64 {
        ConcurrentParser::messages_parsed(self)
    }
    fn total_bytes(&self) -> u64 {
        ConcurrentParser::bytes_processed(self)
    }
    fn total_errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }
    fn worker_count(&self) -> usize {
        self.worker_stats.len()
    }
    fn worker_snapshots(&self) -> Vec<WorkerStatsSnapshot> {
        self.worker_stats()
    }
}

fn num_cpus() -> usize {
    thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

pub struct BatchProcessor {
    parser: Parser,
    batch_size: usize,
    messages: Vec<Message>,
    messages_parsed: AtomicU64,
}

impl BatchProcessor {
    pub fn new(batch_size: usize) -> Self {
        Self {
            parser: Parser::with_capacity(4 * 1024 * 1024),
            batch_size: batch_size.max(1),
            messages: Vec::with_capacity(batch_size),
            messages_parsed: AtomicU64::new(0),
        }
    }

    pub fn process(&mut self, data: &[u8]) -> Result<&[Message]> {
        self.parser.feed_data(data)?;
        self.messages.clear();
        self.messages.reserve(self.batch_size);
        let count = self
            .parser
            .parse_batch(&mut self.messages, self.batch_size)?;
        self.messages_parsed
            .fetch_add(count as u64, Ordering::Relaxed);
        Ok(&self.messages)
    }

    pub fn process_all(&mut self, data: &[u8]) -> Result<Vec<Message>> {
        self.parser.feed_data(data)?;
        let estimated = data.len() / 32;
        let mut all_messages = Vec::with_capacity(estimated);

        loop {
            self.messages.clear();
            let count = self
                .parser
                .parse_batch(&mut self.messages, self.batch_size)?;
            if count == 0 {
                break;
            }
            all_messages.append(&mut self.messages);
            self.messages_parsed
                .fetch_add(count as u64, Ordering::Relaxed);
        }

        Ok(all_messages)
    }

    pub fn messages_parsed(&self) -> u64 {
        self.messages_parsed.load(Ordering::Relaxed)
    }

    pub fn reset(&mut self) {
        self.parser.reset();
        self.messages.clear();
    }
}

impl Default for BatchProcessor {
    fn default() -> Self {
        Self::new(1024)
    }
}

const MIN_BATCH_SIZE: usize = 64;
const MAX_BATCH_SIZE: usize = 16384;
const WINDOW_SIZE: usize = 8;
const GROWTH_FACTOR: f64 = 1.5;
const SHRINK_FACTOR: f64 = 0.75;
const TARGET_BATCH_DURATION_US: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AdaptiveStrategy {
    Latency,
    Throughput,
    #[default]
    Balanced,
    Conservative,
}

#[derive(Debug, Clone)]
pub struct AdaptiveBatchConfig {
    pub min_batch_size: usize,
    pub max_batch_size: usize,
    pub target_latency_us: u64,
    pub target_throughput_mps: Option<f64>,
    pub growth_factor: f64,
    pub shrink_factor: f64,
    pub window_size: usize,
    pub strategy: AdaptiveStrategy,
    pub warmup_batches: usize,
    pub stability_threshold: f64,
    pub enable_backpressure: bool,
    pub max_pending_batches: usize,
}

impl Default for AdaptiveBatchConfig {
    fn default() -> Self {
        Self {
            min_batch_size: MIN_BATCH_SIZE,
            max_batch_size: MAX_BATCH_SIZE,
            target_latency_us: TARGET_BATCH_DURATION_US,
            target_throughput_mps: None,
            growth_factor: GROWTH_FACTOR,
            shrink_factor: SHRINK_FACTOR,
            window_size: WINDOW_SIZE,
            strategy: AdaptiveStrategy::default(),
            warmup_batches: 4,
            stability_threshold: 0.1,
            enable_backpressure: false,
            max_pending_batches: 8,
        }
    }
}

impl AdaptiveBatchConfig {
    pub fn low_latency() -> Self {
        Self {
            min_batch_size: 32,
            max_batch_size: 1024,
            target_latency_us: 50,
            strategy: AdaptiveStrategy::Latency,
            growth_factor: 1.25,
            shrink_factor: 0.8,
            ..Default::default()
        }
    }

    pub fn high_throughput() -> Self {
        Self {
            min_batch_size: 256,
            max_batch_size: 32768,
            target_latency_us: 500,
            strategy: AdaptiveStrategy::Throughput,
            growth_factor: 2.0,
            shrink_factor: 0.5,
            ..Default::default()
        }
    }

    pub fn conservative() -> Self {
        Self {
            strategy: AdaptiveStrategy::Conservative,
            growth_factor: 1.1,
            shrink_factor: 0.95,
            warmup_batches: 8,
            stability_threshold: 0.05,
            ..Default::default()
        }
    }

    pub fn with_latency_target(mut self, latency_us: u64) -> Self {
        self.target_latency_us = latency_us;
        self
    }

    pub fn with_throughput_target(mut self, mps: f64) -> Self {
        self.target_throughput_mps = Some(mps);
        self
    }

    pub fn with_batch_bounds(mut self, min: usize, max: usize) -> Self {
        self.min_batch_size = min.max(1);
        self.max_batch_size = max.max(self.min_batch_size);
        self
    }

    pub fn with_strategy(mut self, strategy: AdaptiveStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_growth_factors(mut self, growth: f64, shrink: f64) -> Self {
        self.growth_factor = growth.max(1.01);
        self.shrink_factor = shrink.clamp(0.01, 0.99);
        self
    }

    pub fn with_window_size(mut self, size: usize) -> Self {
        self.window_size = size.max(1);
        self
    }

    pub fn with_warmup(mut self, batches: usize) -> Self {
        self.warmup_batches = batches;
        self
    }

    pub fn with_backpressure(mut self, max_pending: usize) -> Self {
        self.enable_backpressure = true;
        self.max_pending_batches = max_pending.max(1);
        self
    }
}

pub struct AdaptiveBatchProcessor {
    parser: Parser,
    batch_size: usize,
    min_batch: usize,
    max_batch: usize,
    messages: Vec<Message>,
    throughput_window: [f64; WINDOW_SIZE],
    window_idx: usize,
    window_filled: bool,
    last_batch_time: std::time::Instant,
    total_messages: AtomicU64,
    total_batches: AtomicU64,
    config: AdaptiveBatchConfig,
    start_time: Instant,
    min_batch_observed: usize,
    max_batch_observed: usize,
    latency_histogram: [AtomicU64; 8],
    batch_size_changes: AtomicU64,
    throughput_sum_sq: f64,
    throughput_sum: f64,
    throughput_count: u64,
}

impl AdaptiveBatchProcessor {
    pub fn new() -> Self {
        Self::with_config(AdaptiveBatchConfig::default())
    }

    pub fn with_bounds(min_batch: usize, max_batch: usize) -> Self {
        Self::with_config(AdaptiveBatchConfig::default().with_batch_bounds(min_batch, max_batch))
    }

    pub fn with_config(config: AdaptiveBatchConfig) -> Self {
        let min_batch = config.min_batch_size;
        let max_batch = config.max_batch_size;
        let initial = (min_batch + max_batch) / 2;
        Self {
            parser: Parser::with_capacity(4 * 1024 * 1024),
            batch_size: initial,
            min_batch,
            max_batch,
            messages: Vec::with_capacity(max_batch),
            throughput_window: [0.0; WINDOW_SIZE],
            window_idx: 0,
            window_filled: false,
            last_batch_time: std::time::Instant::now(),
            total_messages: AtomicU64::new(0),
            total_batches: AtomicU64::new(0),
            config,
            start_time: Instant::now(),
            min_batch_observed: initial,
            max_batch_observed: initial,
            latency_histogram: Default::default(),
            batch_size_changes: AtomicU64::new(0),
            throughput_sum_sq: 0.0,
            throughput_sum: 0.0,
            throughput_count: 0,
        }
    }

    fn record_latency(&self, duration: std::time::Duration) {
        let us = duration.as_micros() as u64;
        let bucket = match us {
            0..=9 => 0,
            10..=24 => 1,
            25..=49 => 2,
            50..=99 => 3,
            100..=249 => 4,
            250..=499 => 5,
            500..=999 => 6,
            _ => 7,
        };
        self.latency_histogram[bucket].fetch_add(1, Ordering::Relaxed);
    }

    fn update_batch_bounds(&mut self) {
        if self.batch_size < self.min_batch_observed {
            self.min_batch_observed = self.batch_size;
        }
        if self.batch_size > self.max_batch_observed {
            self.max_batch_observed = self.batch_size;
        }
    }

    pub fn process(&mut self, data: &[u8]) -> Result<&[Message]> {
        self.parser.feed_data(data)?;
        self.messages.clear();
        self.messages.reserve(self.batch_size);

        let start = std::time::Instant::now();
        let count = self
            .parser
            .parse_batch(&mut self.messages, self.batch_size)?;
        let elapsed = start.elapsed();

        if count > 0 {
            self.record_latency(elapsed);
            let throughput = count as f64 / elapsed.as_secs_f64().max(1e-9);
            self.record_throughput(throughput);
            self.throughput_sum += throughput;
            self.throughput_sum_sq += throughput * throughput;
            self.throughput_count += 1;
            let old_batch = self.batch_size;
            self.adapt_batch_size(elapsed);
            if self.batch_size != old_batch {
                self.batch_size_changes.fetch_add(1, Ordering::Relaxed);
            }
            self.update_batch_bounds();
            self.total_messages
                .fetch_add(count as u64, Ordering::Relaxed);
            self.total_batches.fetch_add(1, Ordering::Relaxed);
        }

        self.last_batch_time = std::time::Instant::now();
        Ok(&self.messages)
    }

    pub fn process_all(&mut self, data: &[u8]) -> Result<Vec<Message>> {
        self.parser.feed_data(data)?;
        let estimated = data.len() / 32;
        let mut all_messages = Vec::with_capacity(estimated);

        loop {
            self.messages.clear();
            let start = std::time::Instant::now();
            let count = self
                .parser
                .parse_batch(&mut self.messages, self.batch_size)?;
            let elapsed = start.elapsed();

            if count == 0 {
                break;
            }

            self.record_latency(elapsed);
            let throughput = count as f64 / elapsed.as_secs_f64().max(1e-9);
            self.record_throughput(throughput);
            self.throughput_sum += throughput;
            self.throughput_sum_sq += throughput * throughput;
            self.throughput_count += 1;
            let old_batch = self.batch_size;
            self.adapt_batch_size(elapsed);
            if self.batch_size != old_batch {
                self.batch_size_changes.fetch_add(1, Ordering::Relaxed);
            }
            self.update_batch_bounds();

            all_messages.append(&mut self.messages);
            self.total_messages
                .fetch_add(count as u64, Ordering::Relaxed);
            self.total_batches.fetch_add(1, Ordering::Relaxed);
        }

        Ok(all_messages)
    }

    #[inline]
    fn record_throughput(&mut self, throughput: f64) {
        self.throughput_window[self.window_idx] = throughput;
        self.window_idx = (self.window_idx + 1) % WINDOW_SIZE;
        if self.window_idx == 0 {
            self.window_filled = true;
        }
    }

    #[inline]
    fn moving_avg_throughput(&self) -> f64 {
        let count = if self.window_filled {
            WINDOW_SIZE
        } else {
            self.window_idx.max(1)
        };
        let sum: f64 = self.throughput_window[..count].iter().sum();
        sum / count as f64
    }

    fn throughput_variance(&self) -> f64 {
        if self.throughput_count < 2 {
            return 0.0;
        }
        let n = self.throughput_count as f64;
        let mean = self.throughput_sum / n;
        (self.throughput_sum_sq / n) - (mean * mean)
    }

    #[inline]
    fn adapt_batch_size(&mut self, last_duration: std::time::Duration) {
        let batches = self.total_batches.load(Ordering::Relaxed);
        if batches < self.config.warmup_batches as u64 {
            return;
        }

        match self.config.strategy {
            AdaptiveStrategy::Throughput => self.adapt_for_throughput(last_duration),
            AdaptiveStrategy::Latency => self.adapt_for_latency(last_duration),
            AdaptiveStrategy::Balanced => self.adapt_balanced(last_duration),
            AdaptiveStrategy::Conservative => self.adapt_conservative(last_duration),
        }
    }

    fn adapt_for_throughput(&mut self, last_duration: std::time::Duration) {
        if let Some(target_mps) = self.config.target_throughput_mps {
            let current_mps = self.moving_avg_throughput();
            if current_mps < target_mps * 0.8 && self.batch_size < self.max_batch {
                let new_size = ((self.batch_size as f64) * self.config.growth_factor) as usize;
                self.batch_size = new_size.min(self.max_batch);
            } else if current_mps > target_mps * 1.2 && self.batch_size > self.min_batch {
                let new_size = ((self.batch_size as f64) * self.config.shrink_factor) as usize;
                self.batch_size = new_size.max(self.min_batch);
            }
            return;
        }

        let target_us = self.config.target_latency_us;
        let actual_us = last_duration.as_micros() as u64;

        if actual_us < target_us && self.batch_size < self.max_batch {
            let new_size = ((self.batch_size as f64) * self.config.growth_factor) as usize;
            self.batch_size = new_size.min(self.max_batch);
        }
    }

    fn adapt_for_latency(&mut self, last_duration: std::time::Duration) {
        let target_us = self.config.target_latency_us;
        let actual_us = last_duration.as_micros() as u64;

        if actual_us > target_us && self.batch_size > self.min_batch {
            let new_size = ((self.batch_size as f64) * self.config.shrink_factor) as usize;
            self.batch_size = new_size.max(self.min_batch);
        } else if actual_us < target_us / 4 && self.batch_size < self.max_batch {
            let new_size = ((self.batch_size as f64) * self.config.growth_factor) as usize;
            self.batch_size = new_size.min(self.max_batch);
        }
    }

    fn adapt_balanced(&mut self, last_duration: std::time::Duration) {
        if let Some(target_mps) = self.config.target_throughput_mps {
            let current_mps = self.moving_avg_throughput();
            if current_mps < target_mps * 0.8 && self.batch_size < self.max_batch {
                let new_size = ((self.batch_size as f64) * self.config.growth_factor) as usize;
                self.batch_size = new_size.min(self.max_batch);
            } else if current_mps > target_mps * 1.2 && self.batch_size > self.min_batch {
                let new_size = ((self.batch_size as f64) * self.config.shrink_factor) as usize;
                self.batch_size = new_size.max(self.min_batch);
            }
            return;
        }

        let target_us = self.config.target_latency_us;
        let actual_us = last_duration.as_micros() as u64;

        if actual_us < target_us / 2 && self.batch_size < self.max_batch {
            let new_size = ((self.batch_size as f64) * self.config.growth_factor) as usize;
            self.batch_size = new_size.min(self.max_batch);
        } else if actual_us > target_us * 2 && self.batch_size > self.min_batch {
            let new_size = ((self.batch_size as f64) * self.config.shrink_factor) as usize;
            self.batch_size = new_size.max(self.min_batch);
        }
    }

    fn adapt_conservative(&mut self, last_duration: std::time::Duration) {
        let target_us = self.config.target_latency_us;
        let actual_us = last_duration.as_micros() as u64;
        let variance = self.throughput_variance();
        let avg = self.moving_avg_throughput();

        if avg > 0.0 && variance.sqrt() / avg > self.config.stability_threshold {
            return;
        }

        if actual_us < target_us / 3 && self.batch_size < self.max_batch {
            let new_size = ((self.batch_size as f64) * self.config.growth_factor) as usize;
            self.batch_size = new_size.min(self.max_batch);
        } else if actual_us > target_us * 3 && self.batch_size > self.min_batch {
            let new_size = ((self.batch_size as f64) * self.config.shrink_factor) as usize;
            self.batch_size = new_size.max(self.min_batch);
        }
    }

    pub fn current_batch_size(&self) -> usize {
        self.batch_size
    }

    pub fn config(&self) -> &AdaptiveBatchConfig {
        &self.config
    }

    pub fn metrics(&self) -> AdaptiveBatchMetrics {
        let mut histogram = [0u64; 8];
        for (i, h) in self.latency_histogram.iter().enumerate() {
            histogram[i] = h.load(Ordering::Relaxed);
        }

        AdaptiveBatchMetrics {
            current_batch_size: self.batch_size,
            avg_throughput: self.moving_avg_throughput(),
            total_messages: self.total_messages.load(Ordering::Relaxed),
            total_batches: self.total_batches.load(Ordering::Relaxed),
            elapsed: self.start_time.elapsed(),
            min_batch_observed: self.min_batch_observed,
            max_batch_observed: self.max_batch_observed,
            latency_histogram: histogram,
            batch_size_changes: self.batch_size_changes.load(Ordering::Relaxed),
            strategy: self.config.strategy,
            throughput_variance: self.throughput_variance(),
        }
    }

    pub fn avg_throughput(&self) -> f64 {
        self.moving_avg_throughput()
    }

    pub fn total_messages(&self) -> u64 {
        self.total_messages.load(Ordering::Relaxed)
    }

    pub fn total_batches(&self) -> u64 {
        self.total_batches.load(Ordering::Relaxed)
    }

    pub fn reset(&mut self) {
        self.parser.reset();
        self.messages.clear();
        let initial = (self.min_batch + self.max_batch) / 2;
        self.batch_size = initial;
        self.throughput_window = [0.0; WINDOW_SIZE];
        self.window_idx = 0;
        self.window_filled = false;
        self.min_batch_observed = initial;
        self.max_batch_observed = initial;
        for h in &self.latency_histogram {
            h.store(0, Ordering::Relaxed);
        }
        self.batch_size_changes.store(0, Ordering::Relaxed);
        self.total_messages.store(0, Ordering::Relaxed);
        self.total_batches.store(0, Ordering::Relaxed);
        self.throughput_sum = 0.0;
        self.throughput_sum_sq = 0.0;
        self.throughput_count = 0;
        self.start_time = Instant::now();
    }

    pub fn strategy(&self) -> AdaptiveStrategy {
        self.config.strategy
    }

    pub fn batch_size_range(&self) -> (usize, usize) {
        (self.min_batch_observed, self.max_batch_observed)
    }

    pub fn latency_distribution(&self) -> [u64; 8] {
        let mut result = [0u64; 8];
        for (i, h) in self.latency_histogram.iter().enumerate() {
            result[i] = h.load(Ordering::Relaxed);
        }
        result
    }
}

impl Default for AdaptiveBatchProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveBatchMetrics {
    pub current_batch_size: usize,
    pub avg_throughput: f64,
    pub total_messages: u64,
    pub total_batches: u64,
    pub elapsed: Duration,
    pub min_batch_observed: usize,
    pub max_batch_observed: usize,
    pub latency_histogram: [u64; 8],
    pub batch_size_changes: u64,
    pub strategy: AdaptiveStrategy,
    pub throughput_variance: f64,
}

impl AdaptiveBatchMetrics {
    pub fn throughput_mps(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs > 0.0 {
            self.total_messages as f64 / secs
        } else {
            0.0
        }
    }

    pub fn avg_batch_size(&self) -> f64 {
        if self.total_batches > 0 {
            self.total_messages as f64 / self.total_batches as f64
        } else {
            0.0
        }
    }

    pub fn latency_bucket_name(bucket: usize) -> &'static str {
        match bucket {
            0 => "<10us",
            1 => "10-25us",
            2 => "25-50us",
            3 => "50-100us",
            4 => "100-250us",
            5 => "250-500us",
            6 => "500us-1ms",
            7 => ">1ms",
            _ => "unknown",
        }
    }

    pub fn batch_size_range(&self) -> (usize, usize) {
        (self.min_batch_observed, self.max_batch_observed)
    }

    pub fn stability_score(&self) -> f64 {
        if self.throughput_variance <= 0.0 || self.avg_throughput <= 0.0 {
            return 1.0;
        }
        let cv = self.throughput_variance.sqrt() / self.avg_throughput;
        (1.0 - cv.min(1.0)).max(0.0)
    }

    pub fn efficiency_score(&self) -> f64 {
        if self.total_batches == 0 {
            return 0.0;
        }
        let avg = self.avg_batch_size();
        let range = (self.max_batch_observed - self.min_batch_observed) as f64;
        if range == 0.0 {
            return 1.0;
        }
        1.0 - (range / avg).min(1.0)
    }
}

#[repr(align(64))]
pub struct AtomicStats {
    messages: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
}

impl AtomicStats {
    pub fn new() -> Self {
        Self {
            messages: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn add_messages(&self, count: u64) {
        self.messages.fetch_add(count, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_bytes(&self, count: u64) {
        self.bytes.fetch_add(count, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn messages(&self) -> u64 {
        self.messages.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.messages.store(0, Ordering::Relaxed);
        self.bytes.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
    }

    #[inline]
    pub fn throughput_mps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            self.messages() as f64 / elapsed_secs
        } else {
            0.0
        }
    }

    #[inline]
    pub fn throughput_mbps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            (self.bytes() as f64 / (1024.0 * 1024.0)) / elapsed_secs
        } else {
            0.0
        }
    }
}

#[repr(align(64))]
pub struct WorkerStats {
    pub worker_id: usize,
    pub messages: AtomicU64,
    pub bytes: AtomicU64,
    pub batches: AtomicU64,
    pub errors: AtomicU64,
    pub min_latency_ns: AtomicU64,
    pub max_latency_ns: AtomicU64,
    pub total_latency_ns: AtomicU64,
    pub stall_count: AtomicU64,
    pub steal_count: AtomicU64,
}

impl WorkerStats {
    pub fn new(worker_id: usize) -> Self {
        Self {
            worker_id,
            messages: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            batches: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            stall_count: AtomicU64::new(0),
            steal_count: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn record_batch(&self, messages: u64, bytes: u64) {
        self.messages.fetch_add(messages, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.batches.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_batch_with_latency(&self, messages: u64, bytes: u64, latency_ns: u64) {
        self.record_batch(messages, bytes);
        self.record_latency(latency_ns);
    }

    #[inline(always)]
    pub fn record_latency(&self, latency_ns: u64) {
        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);

        let mut current = self.min_latency_ns.load(Ordering::Relaxed);
        while latency_ns < current {
            match self.min_latency_ns.compare_exchange_weak(
                current,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }

        let mut current = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current {
            match self.max_latency_ns.compare_exchange_weak(
                current,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }
    }

    #[inline(always)]
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_stall(&self) {
        self.stall_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_steal(&self) {
        self.steal_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> WorkerStatsSnapshot {
        let batches = self.batches.load(Ordering::Relaxed);
        let min_lat = self.min_latency_ns.load(Ordering::Relaxed);
        WorkerStatsSnapshot {
            worker_id: self.worker_id,
            messages: self.messages.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            batches,
            errors: self.errors.load(Ordering::Relaxed),
            min_latency_ns: if min_lat == u64::MAX { 0 } else { min_lat },
            max_latency_ns: self.max_latency_ns.load(Ordering::Relaxed),
            avg_latency_ns: if batches > 0 {
                self.total_latency_ns.load(Ordering::Relaxed) / batches
            } else {
                0
            },
            stall_count: self.stall_count.load(Ordering::Relaxed),
            steal_count: self.steal_count.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        self.messages.store(0, Ordering::Relaxed);
        self.bytes.store(0, Ordering::Relaxed);
        self.batches.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        self.min_latency_ns.store(u64::MAX, Ordering::Relaxed);
        self.max_latency_ns.store(0, Ordering::Relaxed);
        self.total_latency_ns.store(0, Ordering::Relaxed);
        self.stall_count.store(0, Ordering::Relaxed);
        self.steal_count.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorkerStatsSnapshot {
    pub worker_id: usize,
    pub messages: u64,
    pub bytes: u64,
    pub batches: u64,
    pub errors: u64,
    pub min_latency_ns: u64,
    pub max_latency_ns: u64,
    pub avg_latency_ns: u64,
    pub stall_count: u64,
    pub steal_count: u64,
}

impl WorkerStatsSnapshot {
    pub fn avg_batch_size(&self) -> f64 {
        if self.batches > 0 {
            self.messages as f64 / self.batches as f64
        } else {
            0.0
        }
    }

    pub fn throughput_mps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            self.messages as f64 / elapsed_secs
        } else {
            0.0
        }
    }

    pub fn throughput_mbps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            (self.bytes as f64 / (1024.0 * 1024.0)) / elapsed_secs
        } else {
            0.0
        }
    }

    pub fn error_rate(&self) -> f64 {
        let total = self.batches + self.errors;
        if total > 0 {
            self.errors as f64 / total as f64
        } else {
            0.0
        }
    }

    pub fn stall_rate(&self) -> f64 {
        let total = self.batches + self.stall_count;
        if total > 0 {
            self.stall_count as f64 / total as f64
        } else {
            0.0
        }
    }
}

impl Default for AtomicStats {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WorkStealingParser {
    workers: Vec<JoinHandle<()>>,
    injector: Arc<crossbeam_deque::Injector<WorkUnit>>,
    #[allow(dead_code)]
    stealers: Vec<crossbeam_deque::Stealer<WorkUnit>>,
    result_sender: Sender<Vec<Message>>,
    result_receiver: Receiver<Vec<Message>>,
    shutdown: Arc<AtomicBool>,
    stats: Arc<AtomicStats>,
    worker_stats: Arc<Vec<WorkerStats>>,
    start_time: Instant,
}

impl WorkStealingParser {
    pub fn new(num_workers: usize) -> Self {
        let num_workers = num_workers.max(1);
        let injector = Arc::new(crossbeam_deque::Injector::new());
        let (result_sender, result_receiver) = bounded::<Vec<Message>>(num_workers * 4);
        let shutdown = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(AtomicStats::new());
        let worker_stats: Arc<Vec<WorkerStats>> =
            Arc::new((0..num_workers).map(WorkerStats::new).collect());

        let mut workers = Vec::with_capacity(num_workers);
        let mut stealers = Vec::with_capacity(num_workers);
        let mut local_queues: Vec<crossbeam_deque::Worker<WorkUnit>> =
            Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let worker = crossbeam_deque::Worker::new_fifo();
            stealers.push(worker.stealer());
            local_queues.push(worker);
        }

        for (worker_id, local) in local_queues.into_iter().enumerate() {
            let injector = Arc::clone(&injector);
            let stealers: Vec<_> = stealers
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != worker_id)
                .map(|(_, s)| s.clone())
                .collect();
            let tx = result_sender.clone();
            let shutdown_flag = Arc::clone(&shutdown);
            let stats_ref = Arc::clone(&stats);
            let ws = Arc::clone(&worker_stats);

            let handle = thread::spawn(move || {
                pin_current_thread();
                let mut parser = Parser::new();

                loop {
                    if shutdown_flag.load(Ordering::Relaxed) {
                        break;
                    }

                    let work = local.pop().or_else(|| {
                        std::iter::repeat_with(|| {
                            injector
                                .steal_batch_and_pop(&local)
                                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
                        })
                        .find(|s| !s.is_retry())
                        .and_then(|s| s.success())
                    });

                    match work {
                        Some(unit) => {
                            match unit {
                                WorkUnit::Owned(data_vec) => {
                                    let data_len = data_vec.len();
                                    match parser.parse_all(&data_vec) {
                                        Ok(messages) => {
                                            let msg_count = messages.len() as u64;
                                            stats_ref.add_messages(msg_count);
                                            stats_ref.add_bytes(data_len as u64);
                                            ws[worker_id].record_batch(msg_count, data_len as u64);
                                            let _ = tx.send(messages);
                                        }
                                        Err(_) => {
                                            stats_ref.add_error();
                                            ws[worker_id].record_error();
                                            let _ = tx.send(Vec::new());
                                        }
                                    }
                                }
                                WorkUnit::ArcSlice(arc, start, end) => {
                                    let slice = &arc[start..end];
                                    let data_len = slice.len();
                                    match parser.parse_all(slice) {
                                        Ok(messages) => {
                                            let msg_count = messages.len() as u64;
                                            stats_ref.add_messages(msg_count);
                                            stats_ref.add_bytes(data_len as u64);
                                            ws[worker_id].record_batch(msg_count, data_len as u64);
                                            let _ = tx.send(messages);
                                        }
                                        Err(_) => {
                                            stats_ref.add_error();
                                            ws[worker_id].record_error();
                                            let _ = tx.send(Vec::new());
                                        }
                                    }
                                }
                            }
                            parser.reset();
                        }
                        None => {
                            std::hint::spin_loop();
                        }
                    }
                }
            });

            workers.push(handle);
        }

        Self {
            workers,
            injector,
            stealers,
            result_sender,
            result_receiver,
            shutdown,
            stats,
            worker_stats,
            start_time: Instant::now(),
        }
    }

    pub fn submit(&self, data: Vec<u8>) {
        self.injector.push(WorkUnit::Owned(data));
    }

    pub fn submit_arc(&self, data: Arc<[u8]>, start: usize, end: usize) {
        if start >= end || end > data.len() {
            return;
        }
        self.injector.push(WorkUnit::ArcSlice(data, start, end));
    }

    pub fn submit_chunks(&self, data: Arc<[u8]>, chunk_size: usize) -> usize {
        let mut count = 0;
        let mut offset = 0;
        while offset < data.len() {
            let end = std::cmp::min(offset + chunk_size, data.len());
            self.injector
                .push(WorkUnit::ArcSlice(Arc::clone(&data), offset, end));
            count += 1;
            offset = end;
        }
        count
    }

    pub fn submit_chunks_arc(&self, data: Arc<[u8]>, chunk_size: usize) -> usize {
        let mut count = 0;
        let mut offset = 0;
        while offset < data.len() {
            let end = std::cmp::min(offset + chunk_size, data.len());
            self.injector
                .push(WorkUnit::ArcSlice(Arc::clone(&data), offset, end));
            count += 1;
            offset = end;
        }
        count
    }

    pub fn recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.recv().ok()
    }

    pub fn try_recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.try_recv().ok()
    }

    pub fn stats(&self) -> &AtomicStats {
        &self.stats
    }

    pub fn pending(&self) -> usize {
        self.injector.len()
    }

    pub fn worker_stats(&self) -> Vec<WorkerStatsSnapshot> {
        self.worker_stats.iter().map(|s| s.snapshot()).collect()
    }

    pub fn num_workers(&self) -> usize {
        self.worker_stats.len()
    }

    pub fn shutdown(self) {
        self.shutdown.store(true, Ordering::Relaxed);
        drop(self.result_sender);
        for worker in self.workers {
            let _ = worker.join();
        }
    }
}

impl Default for WorkStealingParser {
    fn default() -> Self {
        Self::new(num_cpus())
    }
}

impl ConcurrentParser for WorkStealingParser {
    fn submit(&self, data: Vec<u8>) -> Result<()> {
        self.injector.push(WorkUnit::Owned(data));
        Ok(())
    }

    fn recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.recv().ok()
    }

    fn try_recv(&self) -> Option<Vec<Message>> {
        self.result_receiver.try_recv().ok()
    }

    fn pending(&self) -> usize {
        self.injector.len()
    }

    fn messages_parsed(&self) -> u64 {
        self.stats.messages()
    }

    fn bytes_processed(&self) -> u64 {
        self.stats.bytes()
    }
}

impl ParserMetrics for WorkStealingParser {
    fn stats_snapshot(&self) -> ParserStatsSnapshot {
        ParserStatsSnapshot {
            messages: self.stats.messages(),
            bytes: self.stats.bytes(),
            errors: self.stats.errors(),
            batches: self
                .worker_stats
                .iter()
                .map(|s| s.batches.load(Ordering::Relaxed))
                .sum(),
            elapsed: self.start_time.elapsed(),
        }
    }

    fn reset_stats(&self) {
        self.stats.reset();
        for ws in self.worker_stats.iter() {
            ws.reset();
        }
    }
}

impl UnifiedParser for WorkStealingParser {
    fn name(&self) -> &'static str {
        "WorkStealingParser"
    }
    fn submit_data(&self, data: Vec<u8>) -> Result<()> {
        ConcurrentParser::submit(self, data)
    }
    fn receive_messages(&self) -> Option<Vec<Message>> {
        ConcurrentParser::recv(self)
    }
    fn try_receive(&self) -> Option<Vec<Message>> {
        ConcurrentParser::try_recv(self)
    }
    fn pending_count(&self) -> usize {
        ConcurrentParser::pending(self)
    }
    fn total_messages(&self) -> u64 {
        ConcurrentParser::messages_parsed(self)
    }
    fn total_bytes(&self) -> u64 {
        ConcurrentParser::bytes_processed(self)
    }
    fn total_errors(&self) -> u64 {
        self.stats.errors()
    }
    fn worker_count(&self) -> usize {
        self.worker_stats.len()
    }
    fn worker_snapshots(&self) -> Vec<WorkerStatsSnapshot> {
        self.worker_stats()
    }
}
