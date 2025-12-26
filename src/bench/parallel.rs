use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

use crate::{ConcurrentParser, ParallelParser, SpscParser, WorkStealingParser};

use super::utils::{calculate_throughput, find_message_boundary, split_into_chunks};

pub fn bench_parallel(data: &[u8]) -> Result<(u64, f64, f64)> {
    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let chunk_size = (data.len() / num_workers).max(1024 * 1024);
    let parser = ParallelParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();

    let chunks = split_into_chunks(data, chunk_size);

    for (start, end) in &chunks {
        parser.submit_arc(Arc::clone(&data_arc), *start, *end)?;
    }

    let mut total_messages = 0;
    for _ in 0..chunks.len() {
        if let Some(messages) = parser.recv() {
            total_messages += messages.len();
        }
    }

    let wall = t0.elapsed();
    parser.shutdown();

    let (elapsed_ms, mps) = calculate_throughput(total_messages as u64, wall);
    Ok((total_messages as u64, elapsed_ms, mps))
}

pub fn bench_worksteal(data: &[u8]) -> Result<(u64, f64, f64)> {
    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let chunk_size = (data.len() / num_workers).max(512 * 1024);
    let parser = WorkStealingParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();

    let chunks = split_into_chunks(data, chunk_size);

    for (start, end) in &chunks {
        parser
            .submit_arc(Arc::clone(&data_arc), *start, *end)
            .unwrap();
    }

    let mut total_messages = 0;
    for _ in 0..chunks.len() {
        if let Some(messages) = parser.recv() {
            total_messages += messages.len();
        }
    }

    let wall = t0.elapsed();
    let _ = parser.shutdown();

    let (elapsed_ms, mps) = calculate_throughput(total_messages as u64, wall);
    Ok((total_messages as u64, elapsed_ms, mps))
}

pub fn bench_spsc(data: &[u8]) -> Result<(u64, f64, f64)> {
    let parser = Arc::new(SpscParser::new());
    let data_arc: Arc<[u8]> = Arc::from(data);
    let data_for_producer = Arc::clone(&data_arc);
    let parser_producer = Arc::clone(&parser);

    let chunk_size = 512 * 1024;
    let chunks_expected = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let chunks_expected_producer = Arc::clone(&chunks_expected);

    let t0 = Instant::now();

    let producer_handle = std::thread::spawn(move || {
        let mut offset = 0;
        let mut chunks_sent = 0;

        while offset < data_for_producer.len() {
            let end = find_message_boundary(&data_for_producer, offset + chunk_size);

            loop {
                match parser_producer.submit_arc(data_for_producer.clone(), offset, end) {
                    Ok(_) => break,
                    Err(_) => {
                        std::thread::yield_now();
                    }
                }
            }

            chunks_sent += 1;
            offset = end;
        }

        chunks_expected_producer.store(chunks_sent, std::sync::atomic::Ordering::Release);
    });

    let mut total_messages = 0;
    let mut chunks_received = 0;

    loop {
        if let Some(messages) = parser.try_recv() {
            total_messages += messages.len();
            chunks_received += 1;
        }

        let expected = chunks_expected.load(std::sync::atomic::Ordering::Acquire);
        if expected > 0 && chunks_received >= expected {
            break;
        }

        if expected == 0 || chunks_received < expected {
            std::thread::yield_now();
        }
    }

    producer_handle
        .join()
        .map_err(|_| anyhow::anyhow!("Producer thread panicked"))?;

    let wall = t0.elapsed();
    let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;
    Ok((total_messages as u64, wall.as_secs_f64() * 1000.0, mps))
}

pub fn run_parallel(data: &[u8]) -> Result<()> {
    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let chunk_size = (data.len() / num_workers).max(1024 * 1024);
    let parser = ParallelParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();

    let chunks = split_into_chunks(data, chunk_size);
    for (start, end) in &chunks {
        parser.submit_arc(Arc::clone(&data_arc), *start, *end)?;
    }

    let mut total_messages = 0;
    for _ in 0..chunks.len() {
        if let Some(messages) = parser.recv() {
            total_messages += messages.len();
        }
    }

    let wall = t0.elapsed();
    parser.shutdown();

    let mps = total_messages as f64 / wall.as_secs_f64();
    println!(
        "[parallel] Parsed {} messages ({} bytes) in {:?} => {:.2} M msg/sec ({} workers)",
        total_messages,
        data.len(),
        wall,
        mps / 1_000_000.0,
        num_workers
    );

    Ok(())
}

pub fn run_worksteal(data: &[u8]) -> Result<()> {
    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let chunk_size = (data.len() / num_workers).max(512 * 1024);
    let parser = WorkStealingParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();
    let chunks_sent = parser.submit_chunks_arc(data_arc, chunk_size);

    let mut total_messages = 0;
    for _ in 0..chunks_sent {
        if let Some(messages) = parser.recv() {
            total_messages += messages.len();
        }
    }

    let wall = t0.elapsed();
    let stats = parser.stats();
    println!(
        "[worksteal] Parsed {} messages ({} bytes) in {:?} => {:.2} M msg/sec ({} workers)",
        total_messages,
        data.len(),
        wall,
        total_messages as f64 / wall.as_secs_f64() / 1_000_000.0,
        num_workers
    );
    println!(
        "  Stats: {} messages, {} bytes, {} errors",
        stats.messages(),
        stats.bytes(),
        stats.errors()
    );

    let _ = parser.shutdown();
    Ok(())
}

pub fn run_spsc(data: &[u8]) -> Result<()> {
    let parser = Arc::new(SpscParser::new());
    let data_arc: Arc<[u8]> = Arc::from(data);
    let data_for_producer = Arc::clone(&data_arc);
    let parser_producer = Arc::clone(&parser);

    let chunk_size = 512 * 1024;
    let chunks_expected = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let chunks_expected_producer = Arc::clone(&chunks_expected);

    let t0 = Instant::now();

    let producer_handle = std::thread::spawn(move || {
        let mut offset = 0;
        let mut chunks_sent = 0;

        while offset < data_for_producer.len() {
            let end = find_message_boundary(&data_for_producer, offset + chunk_size);

            loop {
                match parser_producer.submit_arc(data_for_producer.clone(), offset, end) {
                    Ok(_) => break,
                    Err(_) => {
                        std::thread::yield_now();
                    }
                }
            }

            chunks_sent += 1;
            offset = end;
        }

        chunks_expected_producer.store(chunks_sent, std::sync::atomic::Ordering::Release);
    });

    let mut total_messages = 0;
    let mut chunks_received = 0;

    loop {
        if let Some(messages) = parser.try_recv() {
            total_messages += messages.len();
            chunks_received += 1;
        }

        let expected = chunks_expected.load(std::sync::atomic::Ordering::Acquire);
        if expected > 0 && chunks_received >= expected {
            break;
        }

        if expected == 0 || chunks_received < expected {
            std::thread::yield_now();
        }
    }

    producer_handle
        .join()
        .map_err(|_| anyhow::anyhow!("Producer thread panicked"))?;

    let wall = t0.elapsed();
    let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;

    println!(
        "[spsc] Parsed {} messages in {:?} => {:.2} M msg/sec",
        total_messages, wall, mps
    );
    println!("  Crossbeam channel SPSC, concurrent producer/consumer");

    Ok(())
}

pub fn run_worker_stats(data: &[u8]) -> Result<()> {
    println!("=== Per-Worker Statistics Benchmark ===");
    println!();

    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let chunk_size = (data.len() / num_workers).max(512 * 1024);
    let parser = WorkStealingParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();
    let chunks_sent = parser.submit_chunks_arc(data_arc, chunk_size);

    let mut total_messages = 0;
    for _ in 0..chunks_sent {
        if let Some(messages) = parser.recv() {
            total_messages += messages.len();
        }
    }

    let wall = t0.elapsed();
    let worker_stats = parser.worker_stats();

    println!(
        "Total: {} messages in {:?} ({} workers)",
        total_messages, wall, num_workers
    );
    println!();

    println!(
        "{:<10} {:>12} {:>12} {:>12} {:>10}",
        "Worker", "Messages", "Bytes", "Errors", "Load %"
    );
    println!("{}", "-".repeat(60));

    let total_worker_msgs: u64 = worker_stats.iter().map(|s| s.messages).sum();

    for (i, stats) in worker_stats.iter().enumerate() {
        let load = if total_worker_msgs > 0 {
            stats.messages as f64 / total_worker_msgs as f64 * 100.0
        } else {
            0.0
        };
        println!(
            "{:<10} {:>12} {:>12} {:>12} {:>9.1}%",
            format!("Worker {}", i),
            stats.messages,
            stats.bytes,
            stats.errors,
            load
        );
    }

    println!();
    let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;
    println!("Throughput: {:.2} M msg/sec", mps);

    let max_load: f64 = worker_stats
        .iter()
        .map(|s| s.messages as f64)
        .fold(0.0_f64, |a, b| a.max(b));
    let min_load: f64 = worker_stats
        .iter()
        .map(|s| s.messages as f64)
        .fold(f64::MAX, |a, b| a.min(b));

    if max_load > 0.0 {
        let balance = min_load / max_load * 100.0;
        println!("Load balance: {:.1}% (100% = perfect)", balance);
    }

    let _ = parser.shutdown();
    Ok(())
}
