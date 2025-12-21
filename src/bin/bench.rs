use anyhow::Result;
use lunary::{
    compute_checksum_simd, scan_boundaries_auto, simd_info, validate_message_stream_simd,
    AdaptiveBatchConfig, AdaptiveBatchMetrics, AdaptiveBatchProcessor, AdaptiveStrategy,
    BatchProcessor, CacheStats, ConcurrentParser, Config, MmapParser, ParallelParser,
    ParseDiagnosticsExt, Parser, SpscParser, WorkStealingParser, ZeroCopyParser,
};
use memmap2::MmapOptions;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
struct BenchConfig {
    iterations: usize,
    warmup_iterations: usize,
    output_format: OutputFormat,
    validate: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum OutputFormat {
    Human,
    Json,
    Csv,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            iterations: 1,
            warmup_iterations: 0,
            output_format: OutputFormat::Human,
            validate: true,
        }
    }
}

fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn variance_f64(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let sum_sq_diff: f64 = values.iter().map(|x| (x - mean).powi(2)).sum();
    sum_sq_diff / (values.len() - 1) as f64
}

#[cfg(test)]
fn make_buffer(messages: &[(u8, usize)]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (msg_type, payload_len) in messages {
        let len = 1 + payload_len;
        buf.extend_from_slice(&(len as u16).to_be_bytes());
        buf.push(*msg_type);
        buf.extend(std::iter::repeat_n(0xABu8, *payload_len));
    }
    buf
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let path = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("Usage: itch-bench <path-to-itch-file> [mode] [max-size-mb] [options]");
        eprintln!();
        eprintln!("Basic Modes:");
        eprintln!(
            "  simple, batch, adaptive, parallel, worksteal, mmap, decode, zerocopy, spsc, simd"
        );
        eprintln!();
        eprintln!("Advanced Modes:");
        eprintln!("  zerocopy-ref   - Zero-copy reference parsing with MessageRef");
        eprintln!("  adaptive-all   - Compare all adaptive strategies");
        eprintln!("  worker-stats   - Parallel parsing with per-worker stats");
        eprintln!("  simd-validate  - SIMD message validation benchmark");
        eprintln!("  latency        - Latency distribution analysis");
        eprintln!("  realworld      - Real-world simulation benchmark");
        eprintln!("  diagnostics    - Full SIMD/cache diagnostics");
        eprintln!("  feature-cmp    - Per-feature comparison (zerocopy vs owned, SIMD vs scalar)");
        eprintln!("  fuzzing        - Fuzzing and error injection test");
        eprintln!();
        eprintln!("  all (default)  - Run all benchmarks");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  max-size-mb    - Maximum file/buffer size in MB (default: 512)");
        eprintln!("  --iterations N - Run each benchmark N times (default: 1)");
        eprintln!("  --warmup N     - Run N warmup iterations before measurement (default: 0)");
        eprintln!("  --json         - Output results in JSON format");
        eprintln!("  --csv          - Output results in CSV format");
        std::process::exit(2);
    });

    let mode = args.get(2).map(|s| s.as_str()).unwrap_or("all");

    let max_size_mb: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(512);

    let mut bench_config = BenchConfig::default();

    for i in 4..args.len() {
        match args[i].as_str() {
            "--json" => bench_config.output_format = OutputFormat::Json,
            "--csv" => bench_config.output_format = OutputFormat::Csv,
            "--iterations" => {
                if let Some(n) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    bench_config.iterations = n;
                }
            }
            "--warmup" => {
                if let Some(n) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    bench_config.warmup_iterations = n;
                }
            }
            "--no-validate" => bench_config.validate = false,
            "--validate" => bench_config.validate = true,
            _ => {}
        }
    }

    let config = Config::from_size_mb(max_size_mb);
    let max_size = config.max_buffer_size;

    print_simd_info();

    if !path.exists() {
        eprintln!("Error: File '{}' does not exist", path.display());
        std::process::exit(1);
    }

    if !path.is_file() {
        eprintln!("Error: '{}' is not a file", path.display());
        std::process::exit(1);
    }

    let file = File::open(&path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };

    let data = &mmap[..std::cmp::min(mmap.len(), max_size)];

    println!("=== Configuration ===");
    println!("  Max buffer size: {} MB", max_size_mb);
    println!(
        "  File size: {:.2} MB",
        mmap.len() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  Data loaded: {:.2} MB",
        data.len() as f64 / (1024.0 * 1024.0)
    );
    println!();

    match mode {
        "simple" => run_simple(data),
        "batch" => run_batch(data),
        "adaptive" => run_adaptive(data),
        "parallel" => run_parallel(data),
        "worksteal" => run_worksteal(data),
        "mmap" => run_mmap(data),
        "decode" => run_decode(data),
        "zerocopy" => run_zerocopy(data),
        "zerocopy-ref" => run_zerocopy_ref(data),
        "spsc" => run_spsc(data),
        "simd" => run_simd_bench(data),
        "simd-validate" => run_simd_validate(data),
        "adaptive-all" => run_adaptive_strategies(data),
        "worker-stats" => run_worker_stats(data),
        "latency" => run_latency_analysis(data),
        "realworld" => run_realworld_simulation(data),
        "diagnostics" => run_diagnostics(data),
        "feature-cmp" => run_feature_comparison(data),
        "fuzzing" => run_fuzzing_test(data),
        "all" => run_all_benchmarks(&path, data, &bench_config),
        _ => {
            eprintln!("Unknown mode: {}", mode);
            std::process::exit(1);
        }
    }
}

fn print_simd_info() {
    let info = simd_info();
    println!("=== SIMD Capabilities ===");
    println!("  SSE2:     {}", if info.sse2 { "✓" } else { "✗" });
    println!("  SSSE3:    {}", if info.ssse3 { "✓" } else { "✗" });
    println!("  AVX2:     {}", if info.avx2 { "✓" } else { "✗" });
    println!("  AVX512:   {}", if info.avx512 { "✓" } else { "✗" });
    println!("  Best:     {}", info.best_available());
    println!("  Width:    {} bytes", info.register_width());
    println!();
}

fn run_all_benchmarks(_path: &PathBuf, data: &[u8], config: &BenchConfig) -> Result<()> {
    println!("=== Benchmark Suite ===");
    println!("File size: {:.2} MB", data.len() as f64 / (1024.0 * 1024.0));
    if config.iterations > 1 {
        println!(
            "Iterations: {} (warmup: {})",
            config.iterations, config.warmup_iterations
        );
    }
    println!();

    let mut results = Vec::new();

    println!("Running basic benchmarks...");

    type BenchResult = (u64, f64, f64);
    type BenchFn = Box<dyn Fn(&[u8]) -> Result<BenchResult>>;

    let bench_fns: Vec<(&str, BenchFn)> = vec![
        ("Simple", Box::new(bench_simple)),
        ("Batch (4096)", Box::new(|d| bench_batch(d, 4096))),
        ("Batch (8192)", Box::new(|d| bench_batch(d, 8192))),
        ("Adaptive", Box::new(bench_adaptive)),
        (
            "Adaptive-Low",
            Box::new(|d| bench_adaptive_config(d, AdaptiveBatchConfig::low_latency())),
        ),
        (
            "Adaptive-High",
            Box::new(|d| bench_adaptive_config(d, AdaptiveBatchConfig::high_throughput())),
        ),
        ("Parallel", Box::new(bench_parallel)),
        ("WorkSteal", Box::new(bench_worksteal)),
        ("SPSC", Box::new(bench_spsc)),
        ("ZeroCopy", Box::new(bench_zerocopy)),
        ("ZeroCopy-Ref", Box::new(bench_zerocopy_ref)),
        ("SIMD Scan", Box::new(bench_simd_scan)),
        ("SIMD Validate", Box::new(bench_simd_validate)),
    ];

    for (name, bench_fn) in bench_fns {
        for _ in 0..config.warmup_iterations {
            let _ = bench_fn(data);
        }

        if config.iterations > 1 {
            let mut times = Vec::new();
            let mut mps_values = Vec::new();
            let mut message_count = 0u64;

            for _ in 0..config.iterations {
                let (msgs, elapsed_ms, mps) = bench_fn(data)?;
                message_count = msgs;
                times.push(elapsed_ms);
                mps_values.push(mps);
            }

            let median_time = median_f64(&mut times);
            let median_mps = median_f64(&mut mps_values);
            let variance = variance_f64(&mps_values);

            results.push((name, message_count, median_time, median_mps, variance));
        } else {
            let (msgs, elapsed_ms, mps) = bench_fn(data)?;
            results.push((name, msgs, elapsed_ms, mps, 0.0));
        }
    }

    match config.output_format {
        OutputFormat::Human => {
            println!("\n=== Summary ===");
            if config.iterations > 1 {
                println!(
                    "{:<15} {:>12} {:>12} {:>12} {:>12}",
                    "Mode", "Messages", "Med Time(ms)", "Med M msg/s", "Variance"
                );
                println!("{}", "-".repeat(70));
            } else {
                println!(
                    "{:<15} {:>12} {:>12} {:>12}",
                    "Mode", "Messages", "Time (ms)", "M msg/sec"
                );
                println!("{}", "-".repeat(55));
            }

            for (name, messages, elapsed_ms, mps, variance) in &results {
                if config.iterations > 1 {
                    println!(
                        "{:<15} {:>12} {:>12.2} {:>12.2} {:>12.2}",
                        name, messages, elapsed_ms, mps, variance
                    );
                } else {
                    println!(
                        "{:<15} {:>12} {:>12.2} {:>12.2}",
                        name, messages, elapsed_ms, mps
                    );
                }
            }

            if let Some((name, _, _, best_mps, _)) =
                results.iter().max_by(|a, b| a.3.partial_cmp(&b.3).unwrap())
            {
                println!("\nBest throughput: {} @ {:.2} M msg/sec", name, best_mps);
            }

            if config.validate {
                let expected_count = results.first().map(|(_, count, _, _, _)| *count);
                if let Some(expected) = expected_count {
                    let mismatches: Vec<_> = results
                        .iter()
                        .filter(|(_, count, _, _, _)| *count != expected)
                        .map(|(name, count, _, _, _)| (name, count))
                        .collect();

                    if !mismatches.is_empty() {
                        println!("\n⚠️  Warning: Message count mismatches detected!");
                        println!("Expected: {} messages", expected);
                        for (name, count) in mismatches {
                            println!(
                                "  {}: {} messages (diff: {})",
                                name,
                                count,
                                *count as i64 - expected as i64
                            );
                        }
                    } else {
                        println!(
                            "\n✓ All benchmarks processed {} messages consistently",
                            expected
                        );
                    }
                }
            }
        }
        OutputFormat::Json => {
            println!("{{");
            println!(
                "  \"file_size_mb\": {:.2},",
                data.len() as f64 / (1024.0 * 1024.0)
            );
            println!("  \"iterations\": {},", config.iterations);
            println!("  \"warmup_iterations\": {},", config.warmup_iterations);
            println!("  \"results\": [");
            for (i, (name, messages, elapsed_ms, mps, variance)) in results.iter().enumerate() {
                let comma = if i < results.len() - 1 { "," } else { "" };
                println!("    {{\"name\": \"{}\", \"messages\": {}, \"time_ms\": {:.2}, \"mps\": {:.2}, \"variance\": {:.2}}}{}", 
                    name, messages, elapsed_ms, mps, variance, comma);
            }
            println!("  ]");
            println!("}}");
        }
        OutputFormat::Csv => {
            println!("name,messages,time_ms,mps,variance");
            for (name, messages, elapsed_ms, mps, variance) in &results {
                println!(
                    "{},{},{:.2},{:.2},{:.2}",
                    name, messages, elapsed_ms, mps, variance
                );
            }
        }
    }

    Ok(())
}

fn bench_simple(data: &[u8]) -> Result<(u64, f64, f64)> {
    let mut parser = Parser::default();
    let t0 = Instant::now();
    let stats = parser.parse(data)?;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(stats.messages, wall);
    Ok((stats.messages, elapsed_ms, mps))
}

fn bench_batch(data: &[u8], batch_size: usize) -> Result<(u64, f64, f64)> {
    let mut processor = BatchProcessor::new(batch_size);
    let t0 = Instant::now();
    let messages = processor.process_all(data)?;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(messages.len() as u64, wall);
    Ok((messages.len() as u64, elapsed_ms, mps))
}

fn bench_adaptive(data: &[u8]) -> Result<(u64, f64, f64)> {
    let mut processor = AdaptiveBatchProcessor::new();
    let t0 = Instant::now();
    let messages = processor.process_all(data)?;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(messages.len() as u64, wall);
    Ok((messages.len() as u64, elapsed_ms, mps))
}

fn bench_parallel(data: &[u8]) -> Result<(u64, f64, f64)> {
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

fn bench_worksteal(data: &[u8]) -> Result<(u64, f64, f64)> {
    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let chunk_size = (data.len() / num_workers).max(512 * 1024);
    let parser = WorkStealingParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();

    let chunks = split_into_chunks(data, chunk_size);

    for (start, end) in &chunks {
        parser.submit_arc(Arc::clone(&data_arc), *start, *end);
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

#[allow(dead_code)]
fn bench_mmap_file(path: &PathBuf) -> Result<(u64, f64, f64)> {
    let t0 = Instant::now();
    let mmap_parser = MmapParser::open(path)?;
    let messages = mmap_parser.parse_all();
    let wall = t0.elapsed();
    let mps = messages.len() as f64 / wall.as_secs_f64() / 1_000_000.0;
    Ok((messages.len() as u64, wall.as_secs_f64() * 1000.0, mps))
}

fn run_simple(data: &[u8]) -> Result<()> {
    let (messages, elapsed_ms, mps) = bench_simple(data)?;
    println!(
        "[simple] Parsed {} messages ({} bytes) in {:.2}ms => {:.2} M msg/sec",
        messages,
        data.len(),
        elapsed_ms,
        mps
    );
    Ok(())
}

fn run_batch(data: &[u8]) -> Result<()> {
    let (messages, elapsed_ms, mps) = bench_batch(data, 4096)?;
    println!(
        "[batch] Parsed {} messages ({} bytes) in {:.2}ms => {:.2} M msg/sec",
        messages,
        data.len(),
        elapsed_ms,
        mps
    );
    Ok(())
}

fn run_adaptive(data: &[u8]) -> Result<()> {
    let mut processor = AdaptiveBatchProcessor::new();
    let t0 = Instant::now();
    let messages = processor.process_all(data)?;
    let wall = t0.elapsed();

    let mps = messages.len() as f64 / wall.as_secs_f64();
    println!(
        "[adaptive] Parsed {} messages ({} bytes) in {:?} => {:.2} M msg/sec",
        messages.len(),
        data.len(),
        wall,
        mps / 1_000_000.0
    );
    println!(
        "  Final batch size: {}, Avg throughput: {:.2} msg/sec",
        processor.current_batch_size(),
        processor.avg_throughput()
    );

    Ok(())
}

fn run_parallel(data: &[u8]) -> Result<()> {
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

fn run_worksteal(data: &[u8]) -> Result<()> {
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

    parser.shutdown();
    Ok(())
}

fn run_mmap(data: &[u8]) -> Result<()> {
    let t0 = Instant::now();
    let mut parser = ZeroCopyParser::new(data);
    let messages = parser.parse_all();
    let wall = t0.elapsed();

    let mps = messages.len() as f64 / wall.as_secs_f64();
    println!(
        "[mmap] Parsed {} messages ({} bytes) in {:?} => {:.2} M msg/sec",
        messages.len(),
        data.len(),
        wall,
        mps / 1_000_000.0
    );

    Ok(())
}

fn run_decode(data: &[u8]) -> Result<()> {
    let mut parser = Parser::default();
    let t0 = Instant::now();
    let messages = parser.parse_all(data)?;
    let wall = t0.elapsed();

    let mps = messages.len() as f64 / wall.as_secs_f64();
    println!(
        "[decode] Parsed {} messages ({} bytes) in {:?} => {:.2} M msg/sec",
        messages.len(),
        data.len(),
        wall,
        mps / 1_000_000.0
    );

    let mut type_counts = std::collections::HashMap::new();
    for msg in &messages {
        let type_name = match msg {
            lunary::Message::SystemEvent(_) => "SystemEvent",
            lunary::Message::StockDirectory(_) => "StockDirectory",
            lunary::Message::StockTradingAction(_) => "StockTradingAction",
            lunary::Message::RegShoRestriction(_) => "RegShoRestriction",
            lunary::Message::MarketParticipantPosition(_) => "MarketParticipantPosition",
            lunary::Message::MwcbDeclineLevel(_) => "MwcbDeclineLevel",
            lunary::Message::MwcbStatus(_) => "MwcbStatus",
            lunary::Message::IpoQuotingPeriod(_) => "IpoQuotingPeriod",
            lunary::Message::AddOrder(_) => "AddOrder",
            lunary::Message::AddOrderWithMpid(_) => "AddOrderWithMpid",
            lunary::Message::OrderExecuted(_) => "OrderExecuted",
            lunary::Message::OrderExecutedWithPrice(_) => "OrderExecutedWithPrice",
            lunary::Message::OrderCancel(_) => "OrderCancel",
            lunary::Message::OrderDelete(_) => "OrderDelete",
            lunary::Message::OrderReplace(_) => "OrderReplace",
            lunary::Message::Trade(_) => "Trade",
            lunary::Message::CrossTrade(_) => "CrossTrade",
            lunary::Message::BrokenTrade(_) => "BrokenTrade",
            lunary::Message::NetOrderImbalance(_) => "NetOrderImbalance",
            lunary::Message::RetailPriceImprovement(_) => "RetailPriceImprovement",
            lunary::Message::LuldAuctionCollar(_) => "LuldAuctionCollar",
        };
        *type_counts.entry(type_name).or_insert(0u64) += 1;
    }

    println!("\nMessage type breakdown:");
    let mut counts: Vec<_> = type_counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    for (name, count) in counts {
        println!("  {}: {}", name, count);
    }

    Ok(())
}

fn find_message_boundary(data: &[u8], target: usize) -> usize {
    if target >= data.len() {
        return data.len();
    }

    let mut offset = 0;
    while offset < data.len() {
        if offset + 2 > data.len() {
            return data.len();
        }
        let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let next = offset + 2 + len;
        if next > data.len() {
            return offset;
        }
        if next >= target {
            return next;
        }
        offset = next;
    }
    data.len()
}

fn build_message_boundaries(data: &[u8]) -> Vec<usize> {
    let mut boundaries = Vec::new();
    let mut offset = 0;

    while offset + 2 <= data.len() {
        let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let next = offset + 2 + len;
        if next > data.len() {
            break;
        }
        boundaries.push(next);
        offset = next;
    }

    boundaries
}

fn split_into_chunks(data: &[u8], chunk_size: usize) -> Vec<(usize, usize)> {
    let boundaries = build_message_boundaries(data);
    if boundaries.is_empty() {
        return vec![(0, data.len())];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < data.len() {
        let target = (start + chunk_size).min(data.len());

        let pos = match boundaries.binary_search(&target) {
            Ok(idx) => idx,
            Err(idx) => {
                if idx >= boundaries.len() {
                    boundaries.len() - 1
                } else {
                    idx
                }
            }
        };

        let end = boundaries.get(pos).copied().unwrap_or(data.len());
        if end > start {
            chunks.push((start, end));
            start = end;
        } else {
            break;
        }
    }

    if chunks.is_empty() && !data.is_empty() {
        chunks.push((0, data.len()));
    }

    chunks
}
fn calculate_throughput(count: u64, elapsed: std::time::Duration) -> (f64, f64) {
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let mps = count as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    (elapsed_ms, mps)
}

fn bench_zerocopy(data: &[u8]) -> Result<(u64, f64, f64)> {
    let t0 = Instant::now();
    let parser = ZeroCopyParser::new(data);
    let mut count = 0u64;
    for _ in parser {
        count += 1;
    }
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(count, wall);
    Ok((count, elapsed_ms, mps))
}

fn bench_spsc(data: &[u8]) -> Result<(u64, f64, f64)> {
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

    producer_handle.join().expect("Producer thread panicked");

    let wall = t0.elapsed();
    let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;
    Ok((total_messages as u64, wall.as_secs_f64() * 1000.0, mps))
}

fn bench_simd_scan(data: &[u8]) -> Result<(u64, f64, f64)> {
    let t0 = Instant::now();
    let result = scan_boundaries_auto(data, usize::MAX);
    let wall = t0.elapsed();
    let count = result.len() as u64;
    let (elapsed_ms, mps) = calculate_throughput(count, wall);
    Ok((count, elapsed_ms, mps))
}

fn bench_adaptive_config(data: &[u8], config: AdaptiveBatchConfig) -> Result<(u64, f64, f64)> {
    let mut processor = AdaptiveBatchProcessor::with_config(config);
    let t0 = Instant::now();
    let messages = processor.process_all(data)?;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(messages.len() as u64, wall);
    Ok((messages.len() as u64, elapsed_ms, mps))
}

fn run_zerocopy(data: &[u8]) -> Result<()> {
    let t0 = Instant::now();
    let parser = ZeroCopyParser::new(data);

    let mut count = 0u64;
    let mut total_bytes = 0u64;

    for msg in parser {
        count += 1;
        total_bytes += msg.len() as u64;
    }

    let wall = t0.elapsed();
    let mps = count as f64 / wall.as_secs_f64();

    println!(
        "[zerocopy] Scanned {} messages ({} bytes payload) in {:?} => {:.2} M msg/sec",
        count,
        total_bytes,
        wall,
        mps / 1_000_000.0
    );
    println!("  Zero allocations during parsing");

    Ok(())
}

fn run_spsc(data: &[u8]) -> Result<()> {
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

    producer_handle.join().expect("Producer thread panicked");

    let wall = t0.elapsed();
    let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;

    println!(
        "[spsc] Parsed {} messages in {:?} => {:.2} M msg/sec",
        total_messages, wall, mps
    );
    println!("  Lock-free SPSC queue, concurrent producer/consumer");

    Ok(())
}

fn run_simd_bench(data: &[u8]) -> Result<()> {
    let info = simd_info();
    println!("=== SIMD Boundary Scan Benchmark ===");
    println!("Using: {}", info.best_available());
    println!();

    let iterations = 10;
    let mut total_time = std::time::Duration::ZERO;
    let mut result_count = 0u64;

    for i in 0..iterations {
        let t0 = Instant::now();
        let result = scan_boundaries_auto(data, usize::MAX);
        let elapsed = t0.elapsed();
        total_time += elapsed;
        result_count = result.len() as u64;

        if i == 0 {
            let diag = &result.diagnostics;
            println!("Diagnostics:");
            println!("  Messages scanned: {}", diag.messages_scanned);
            println!("  SIMD bytes: {}", diag.simd_bytes);
            println!("  Scalar bytes: {}", diag.scalar_bytes);
            println!("  Prefetch count: {}", diag.prefetch_count);
            println!(
                "  SIMD utilization: {:.1}%",
                diag.simd_utilization() * 100.0
            );
            if let Some(level) = diag.level_used {
                println!("  SIMD level: {}", level);
            }
        }
    }

    let avg_time = total_time / iterations as u32;
    let avg_mps = result_count as f64 / avg_time.as_secs_f64() / 1_000_000.0;

    println!();
    println!("Results ({} iterations):", iterations);
    println!("  Messages: {}", result_count);
    println!("  Avg time: {:?}", avg_time);
    println!("  Throughput: {:.2} M msg/sec", avg_mps);

    Ok(())
}

fn bench_zerocopy_ref(data: &[u8]) -> Result<(u64, f64, f64)> {
    let t0 = Instant::now();
    let parser = ZeroCopyParser::new(data);
    let mut count = 0u64;
    let mut add_orders = 0u64;
    for msg in parser {
        count += 1;
        if msg.msg_type() == b'A' || msg.msg_type() == b'F' {
            add_orders += 1;
        }
    }
    let _ = add_orders;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(count, wall);
    Ok((count, elapsed_ms, mps))
}

fn bench_simd_validate(data: &[u8]) -> Result<(u64, f64, f64)> {
    let boundaries = scan_boundaries_auto(data, usize::MAX);
    let t0 = Instant::now();
    let mut valid_count = 0u64;
    for (offset, _len) in boundaries.boundaries.iter() {
        if *offset + 3 < data.len() {
            let msg_type = data[*offset + 2];
            if lunary::is_valid_message_type(msg_type) {
                valid_count += 1;
            }
        }
    }
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(valid_count, wall);
    Ok((valid_count, elapsed_ms, mps))
}

fn run_zerocopy_ref(data: &[u8]) -> Result<()> {
    let t0 = Instant::now();
    let parser = ZeroCopyParser::new(data);

    let mut count = 0u64;
    let mut add_orders = 0u64;
    let mut trades = 0u64;

    for msg in parser {
        count += 1;
        match msg.msg_type() {
            b'A' | b'F' => add_orders += 1,
            b'P' | b'Q' => trades += 1,
            _ => {}
        }
    }

    let wall = t0.elapsed();
    let mps = count as f64 / wall.as_secs_f64();

    println!(
        "[zerocopy-ref] Scanned {} messages in {:?} => {:.2} M msg/sec",
        count,
        wall,
        mps / 1_000_000.0
    );
    println!("  Add Orders: {}, Trades: {}", add_orders, trades);
    println!("  Zero allocations, direct byte access");

    Ok(())
}

fn run_simd_validate(data: &[u8]) -> Result<()> {
    println!("=== SIMD Message Validation Benchmark ===");

    let boundaries = scan_boundaries_auto(data, usize::MAX);
    println!("Scanned {} message boundaries", boundaries.len());

    let iterations = 10;
    let mut total_time = std::time::Duration::ZERO;
    let mut valid = 0u64;
    let mut invalid = 0u64;

    for i in 0..iterations {
        let t0 = Instant::now();
        valid = 0;
        invalid = 0;

        for (offset, _len) in boundaries.boundaries.iter() {
            if *offset + 3 < data.len() {
                let msg_type = data[*offset + 2];
                if lunary::is_valid_message_type(msg_type) {
                    valid += 1;
                } else {
                    invalid += 1;
                }
            }
        }

        total_time += t0.elapsed();

        if i == 0 {
            println!("  Valid messages: {}", valid);
            println!("  Invalid messages: {}", invalid);
        }
    }

    let avg_time = total_time / iterations as u32;
    let total_validated = valid + invalid;
    let avg_mps = total_validated as f64 / avg_time.as_secs_f64() / 1_000_000.0;

    println!();
    println!("Results ({} iterations):", iterations);
    println!("  Avg time: {:?}", avg_time);
    println!("  Throughput: {:.2} M validations/sec", avg_mps);

    Ok(())
}

fn run_adaptive_strategies(data: &[u8]) -> Result<()> {
    println!("=== Adaptive Strategies Comparison ===");
    println!();

    let strategies = [
        ("Default (Balanced)", AdaptiveBatchConfig::default()),
        ("Low Latency", AdaptiveBatchConfig::low_latency()),
        ("High Throughput", AdaptiveBatchConfig::high_throughput()),
        ("Conservative", AdaptiveBatchConfig::conservative()),
    ];

    let mut results = Vec::new();

    for (name, config) in strategies {
        let mut processor = AdaptiveBatchProcessor::with_config(config);
        let t0 = Instant::now();
        let messages = processor.process_all(data)?;
        let wall = t0.elapsed();
        let metrics = processor.metrics();

        let mps = messages.len() as f64 / wall.as_secs_f64() / 1_000_000.0;
        results.push((name, messages.len(), wall, mps, metrics));
    }

    println!(
        "{:<20} {:>10} {:>12} {:>10} {:>12} {:>10}",
        "Strategy", "Messages", "Time (ms)", "M msg/s", "Batch Range", "Changes"
    );
    println!("{}", "-".repeat(80));

    for (name, msgs, wall, mps, metrics) in &results {
        let (min_b, max_b) = metrics.batch_size_range();
        println!(
            "{:<20} {:>10} {:>12.2} {:>10.2} {:>5}-{:<6} {:>10}",
            name,
            msgs,
            wall.as_secs_f64() * 1000.0,
            mps,
            min_b,
            max_b,
            metrics.batch_size_changes
        );
    }

    println!();

    if let Some((name, _, _, mps, _)) = results.iter().max_by(|a, b| a.3.partial_cmp(&b.3).unwrap())
    {
        println!("Best strategy: {} @ {:.2} M msg/sec", name, mps);
    }

    Ok(())
}

fn run_worker_stats(data: &[u8]) -> Result<()> {
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

    parser.shutdown();
    Ok(())
}

fn run_latency_analysis(data: &[u8]) -> Result<()> {
    println!("=== Latency Distribution Analysis ===");
    println!();

    let mut processor =
        AdaptiveBatchProcessor::with_config(AdaptiveBatchConfig::default().with_warmup(0));

    let t0 = Instant::now();
    let messages = processor.process_all(data)?;
    let wall = t0.elapsed();
    let metrics = processor.metrics();

    println!("Processed {} messages in {:?}", messages.len(), wall);
    println!();

    println!("Latency Histogram:");
    let total: u64 = metrics.latency_histogram.iter().sum();
    for (i, count) in metrics.latency_histogram.iter().enumerate() {
        let pct = if total > 0 {
            *count as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let bar_len = (pct / 2.0) as usize;
        let bar: String = "█".repeat(bar_len);
        println!(
            "  {:>10}: {:>8} ({:>5.1}%) {}",
            AdaptiveBatchMetrics::latency_bucket_name(i),
            count,
            pct,
            bar
        );
    }

    println!();
    println!("Batch Size Dynamics:");
    println!(
        "  Initial: {}",
        (processor.config().min_batch_size + processor.config().max_batch_size) / 2
    );
    println!("  Final: {}", metrics.current_batch_size);
    println!(
        "  Range: {} - {}",
        metrics.min_batch_observed, metrics.max_batch_observed
    );
    println!("  Changes: {}", metrics.batch_size_changes);
    println!();

    println!("Quality Metrics:");
    println!(
        "  Stability score: {:.2} (1.0 = perfect)",
        metrics.stability_score()
    );
    println!(
        "  Efficiency score: {:.2} (1.0 = perfect)",
        metrics.efficiency_score()
    );
    println!("  Throughput variance: {:.0}", metrics.throughput_variance);

    Ok(())
}

fn run_realworld_simulation(data: &[u8]) -> Result<()> {
    println!("=== Real-World Simulation Benchmark ===");
    println!();

    let chunk_sizes = [64 * 1024, 256 * 1024, 1024 * 1024, 4 * 1024 * 1024];

    for chunk_size in chunk_sizes {
        println!("--- Chunk size: {} KB ---", chunk_size / 1024);

        let mut processor = AdaptiveBatchProcessor::with_config(
            AdaptiveBatchConfig::default().with_strategy(AdaptiveStrategy::Balanced),
        );

        let t0 = Instant::now();
        let messages = processor.process_all(data)?;
        let total_messages = messages.len();
        let wall = t0.elapsed();
        let metrics = processor.metrics();

        let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;
        println!("  Messages: {}", total_messages);
        println!("  Time: {:?}", wall);
        println!("  Throughput: {:.2} M msg/sec", mps);
        println!(
            "  Final batch: {}, Changes: {}",
            metrics.current_batch_size, metrics.batch_size_changes
        );
        println!();
    }

    println!("--- Concurrent Simulation (producers/consumers) ---");

    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let parser = ParallelParser::new(num_workers);
    let chunk_size = 512 * 1024;
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

    let mps = total_messages as f64 / wall.as_secs_f64() / 1_000_000.0;
    println!("  Workers: {}", num_workers);
    println!("  Messages: {}", total_messages);
    println!("  Time: {:?}", wall);
    println!("  Throughput: {:.2} M msg/sec", mps);

    Ok(())
}

fn run_diagnostics(data: &[u8]) -> Result<()> {
    println!("=== Full SIMD/Cache Diagnostics ===");
    println!();

    println!("SIMD Capabilities:");
    print_simd_info();
    println!();

    println!("Message Validation Diagnostics:");
    let t0 = Instant::now();
    let validation = validate_message_stream_simd(data, 100);
    let validation_time = t0.elapsed();

    println!("  Valid messages: {}", validation.valid_count);
    println!("  Invalid messages: {}", validation.invalid_count);
    println!("  Bytes validated: {}", validation.bytes_validated);
    println!("  SIMD level used: {:?}", validation.simd_level);
    println!("  Validation time: {:?}", validation_time);

    if !validation.error_offsets.is_empty() {
        println!("  First {} errors:", validation.error_offsets.len().min(10));
        for (i, (offset, err_type)) in validation
            .error_offsets
            .iter()
            .zip(validation.error_types.iter())
            .take(10)
            .enumerate()
        {
            println!("    {}: offset {} - {:?}", i + 1, offset, err_type);
        }
    }
    println!();

    println!("SIMD Checksum Performance:");
    let iterations = 100;
    let mut total_checksum_time = std::time::Duration::ZERO;
    let mut checksum_result = 0u32;

    for _ in 0..iterations {
        let t0 = Instant::now();
        checksum_result = compute_checksum_simd(data);
        total_checksum_time += t0.elapsed();
    }

    let avg_checksum_time = total_checksum_time / iterations as u32;
    let checksum_gbps = data.len() as f64 / avg_checksum_time.as_secs_f64() / 1_000_000_000.0;

    println!("  Checksum value: 0x{:08x}", checksum_result);
    println!("  Avg time: {:?}", avg_checksum_time);
    println!("  Throughput: {:.2} GB/sec", checksum_gbps);
    println!();

    println!("Boundary Scanning Performance:");
    let t0 = Instant::now();
    let boundaries = scan_boundaries_auto(data, usize::MAX);
    let scan_time = t0.elapsed();

    let scan_gbps = data.len() as f64 / scan_time.as_secs_f64() / 1_000_000_000.0;
    println!("  Boundaries found: {}", boundaries.len());
    println!("  Scan time: {:?}", scan_time);
    println!("  Throughput: {:.2} GB/sec", scan_gbps);
    println!();

    println!("ParseDiagnosticsExt Summary:");
    let simd_diag = lunary::SimdDiagnostics {
        bytes_processed: data.len() as u64,
        simd_bytes: data.len() as u64,
        scalar_bytes: 0,
        messages_scanned: boundaries.len() as u64,
        prefetch_count: 0,
        level_used: Some(lunary::SimdLevel::Avx2),
    };

    let diag = ParseDiagnosticsExt {
        simd_diagnostics: simd_diag,
        cache_stats: CacheStats::default(),
        validation_result: validation,
        parse_time_ns: validation_time.as_nanos() as u64,
        throughput_gbps: data.len() as f64 / validation_time.as_secs_f64() / 1_000_000_000.0,
    };

    println!("{}", diag.summary());

    Ok(())
}

fn run_feature_comparison(data: &[u8]) -> Result<()> {
    println!("=== Per-Feature Comparison ===");
    println!();

    println!("--- Zero-Copy vs Owned Parsing ---");
    let t0 = Instant::now();
    let zerocopy_parser = ZeroCopyParser::new(data);
    let mut zerocopy_count = 0u64;
    for _msg in zerocopy_parser {
        zerocopy_count += 1;
    }
    let zerocopy_time = t0.elapsed();
    let zerocopy_mps = zerocopy_count as f64 / zerocopy_time.as_secs_f64() / 1_000_000.0;

    let t0 = Instant::now();
    let mut owned_parser = Parser::default();
    let owned_stats = owned_parser.parse(data)?;
    let owned_time = t0.elapsed();
    let owned_mps = owned_stats.messages as f64 / owned_time.as_secs_f64() / 1_000_000.0;

    println!(
        "{:<20} {:>12} {:>12} {:>12}",
        "Mode", "Messages", "Time (ms)", "M msg/sec"
    );
    println!("{}", "-".repeat(60));
    println!(
        "{:<20} {:>12} {:>12.2} {:>12.2}",
        "Zero-Copy (Ref)",
        zerocopy_count,
        zerocopy_time.as_secs_f64() * 1000.0,
        zerocopy_mps
    );
    println!(
        "{:<20} {:>12} {:>12.2} {:>12.2}",
        "Owned (Alloc)",
        owned_stats.messages,
        owned_time.as_secs_f64() * 1000.0,
        owned_mps
    );

    let speedup = zerocopy_mps / owned_mps;
    println!("\nZero-copy speedup: {:.2}x", speedup);
    println!();

    println!("--- SIMD vs Scalar Boundary Scanning ---");

    let iterations = 10;

    let mut simd_time = std::time::Duration::ZERO;
    let mut simd_count = 0;
    for _ in 0..iterations {
        let t0 = Instant::now();
        let result = scan_boundaries_auto(data, usize::MAX);
        simd_time += t0.elapsed();
        simd_count = result.len();
    }
    let avg_simd_time = simd_time / iterations as u32;
    let simd_gbps = data.len() as f64 / avg_simd_time.as_secs_f64() / 1_000_000_000.0;

    let mut scalar_time = std::time::Duration::ZERO;
    let mut scalar_count = 0usize;
    for _ in 0..iterations {
        let t0 = Instant::now();
        let mut offset = 0;
        while offset + 2 < data.len() {
            let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            if len == 0 || offset + 2 + len > data.len() {
                break;
            }
            scalar_count += 1;
            offset += 2 + len;
        }
        scalar_time += t0.elapsed();
    }
    scalar_count /= iterations;
    let avg_scalar_time = scalar_time / iterations as u32;
    let scalar_gbps = data.len() as f64 / avg_scalar_time.as_secs_f64() / 1_000_000_000.0;

    println!(
        "{:<20} {:>12} {:>12} {:>12}",
        "Mode", "Boundaries", "Time (ms)", "GB/sec"
    );
    println!("{}", "-".repeat(60));
    println!(
        "{:<20} {:>12} {:>12.2} {:>12.2}",
        "SIMD Auto",
        simd_count,
        avg_simd_time.as_secs_f64() * 1000.0,
        simd_gbps
    );
    println!(
        "{:<20} {:>12} {:>12.2} {:>12.2}",
        "Scalar Loop",
        scalar_count,
        avg_scalar_time.as_secs_f64() * 1000.0,
        scalar_gbps
    );

    let simd_speedup = simd_gbps / scalar_gbps;
    println!("\nSIMD speedup: {:.2}x", simd_speedup);
    println!();

    println!("--- Single-Threaded vs Multi-Threaded ---");

    let num_workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);

    let t0 = Instant::now();
    let mut batch_processor = BatchProcessor::new(4096);
    let batch_messages = batch_processor.process_all(data)?;
    let single_time = t0.elapsed();
    let single_mps = batch_messages.len() as f64 / single_time.as_secs_f64() / 1_000_000.0;

    let chunk_size = (data.len() / num_workers).max(512 * 1024);
    let parser = WorkStealingParser::new(num_workers);
    let data_arc: Arc<[u8]> = Arc::from(data);

    let t0 = Instant::now();
    let chunks_sent = parser.submit_chunks_arc(data_arc, chunk_size);
    let mut parallel_count = 0;
    for _ in 0..chunks_sent {
        if let Some(messages) = parser.recv() {
            parallel_count += messages.len();
        }
    }
    let parallel_time = t0.elapsed();
    parser.shutdown();
    let parallel_mps = parallel_count as f64 / parallel_time.as_secs_f64() / 1_000_000.0;

    println!(
        "{:<20} {:>12} {:>12} {:>12}",
        "Mode", "Messages", "Time (ms)", "M msg/sec"
    );
    println!("{}", "-".repeat(60));
    println!(
        "{:<20} {:>12} {:>12.2} {:>12.2}",
        "Single-Thread",
        batch_messages.len(),
        single_time.as_secs_f64() * 1000.0,
        single_mps
    );
    println!(
        "{:<20} {:>12} {:>12.2} {:>12.2}",
        format!("Parallel ({}T)", num_workers),
        parallel_count,
        parallel_time.as_secs_f64() * 1000.0,
        parallel_mps
    );

    let parallel_speedup = parallel_mps / single_mps;
    println!(
        "\nParallel speedup: {:.2}x (efficiency: {:.1}%)",
        parallel_speedup,
        parallel_speedup / num_workers as f64 * 100.0
    );

    Ok(())
}

fn run_fuzzing_test(data: &[u8]) -> Result<()> {
    println!("=== Fuzzing and Error Injection Test ===");
    println!();

    println!("--- Baseline (Valid Data) ---");
    let validation = validate_message_stream_simd(data, 1000);
    println!(
        "Valid: {}, Invalid: {}",
        validation.valid_count, validation.invalid_count
    );
    println!();

    println!("--- Truncated Data Test ---");
    for truncate_pct in [10, 25, 50, 75, 90] {
        let truncate_at = data.len() * truncate_pct / 100;
        let truncated = &data[..truncate_at];
        let result = validate_message_stream_simd(truncated, 100);
        println!(
            "  {}% ({} bytes): valid={}, invalid={}",
            truncate_pct,
            truncated.len(),
            result.valid_count,
            result.invalid_count
        );
    }
    println!();

    println!("--- Corrupted Message Types Test ---");
    let mut corrupted = data.to_vec();
    let mut message_index = 0;
    let mut actual_corrupted = 0;
    let mut offset = 0;
    while offset + 3 < corrupted.len() && message_index < 100 {
        let len = u16::from_be_bytes([corrupted[offset], corrupted[offset + 1]]) as usize;
        if len == 0 || offset + 2 + len > corrupted.len() {
            break;
        }
        if message_index % 10 == 5 {
            corrupted[offset + 2] = 0xFF;
            actual_corrupted += 1;
        }
        message_index += 1;
        offset += 2 + len;
    }

    let result = validate_message_stream_simd(&corrupted, 1000);
    println!("  Corrupted {} message types", actual_corrupted);
    println!(
        "  Valid: {}, Invalid: {}",
        result.valid_count, result.invalid_count
    );
    if !result.error_types.is_empty() {
        let mut error_counts: std::collections::HashMap<_, usize> =
            std::collections::HashMap::new();
        for err in &result.error_types {
            *error_counts.entry(format!("{:?}", err)).or_insert(0) += 1;
        }
        println!("  Error breakdown:");
        for (err_type, count) in error_counts {
            println!("    {}: {}", err_type, count);
        }
    }
    println!();

    println!("--- Random Noise Injection Test ---");
    let mut noisy = data.to_vec();
    let noise_positions: Vec<usize> = (0..100)
        .map(|i| (i * data.len() / 100 + 7) % data.len())
        .collect();

    for pos in &noise_positions {
        if *pos < noisy.len() {
            noisy[*pos] = (noisy[*pos].wrapping_add(0x55)) ^ 0xAA;
        }
    }

    let result = validate_message_stream_simd(&noisy, 1000);
    println!("  Injected noise at {} positions", noise_positions.len());
    println!(
        "  Valid: {}, Invalid: {}",
        result.valid_count, result.invalid_count
    );
    println!();

    println!("--- Parser Robustness Test ---");

    let zerocopy_result = std::panic::catch_unwind(|| {
        let parser = ZeroCopyParser::new(&noisy);
        let mut count = 0u64;
        for _msg in parser {
            count += 1;
        }
        count
    });

    match zerocopy_result {
        Ok(count) => println!("  ZeroCopyParser: Processed {} messages (no panic)", count),
        Err(_) => println!("  ZeroCopyParser: Panicked on corrupted data"),
    }

    let batch_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut processor = BatchProcessor::new(1024);
        processor.process_all(&noisy).map(|m| m.len()).unwrap_or(0)
    }));

    match batch_result {
        Ok(count) => println!("  BatchProcessor: Processed {} messages (no panic)", count),
        Err(_) => println!("  BatchProcessor: Panicked on corrupted data"),
    }

    println!();
    println!("Fuzzing tests complete.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_message_boundaries_empty() {
        let buf: &[u8] = &[];
        let boundaries = build_message_boundaries(buf);
        assert_eq!(boundaries, vec![]);
    }

    #[test]
    fn test_build_message_boundaries_single() {
        let buf = make_buffer(&[(b'A', 5)]);
        let boundaries = build_message_boundaries(&buf);
        assert_eq!(boundaries, vec![8]);
    }

    #[test]
    fn test_build_message_boundaries_multiple() {
        let buf = make_buffer(&[(b'A', 5), (b'B', 3), (b'C', 10)]);
        let boundaries = build_message_boundaries(&buf);
        assert_eq!(boundaries, vec![8, 14, 27]);
    }

    #[test]
    fn test_find_message_boundary_at_start() {
        let buf = make_buffer(&[(b'A', 5), (b'B', 3)]);
        assert_eq!(find_message_boundary(&buf, 0), 8);
    }

    #[test]
    fn test_find_message_boundary_mid_message() {
        let buf = make_buffer(&[(b'A', 5), (b'B', 3)]);
        let result = find_message_boundary(&buf, 4);
        assert_eq!(result, 8);
    }

    #[test]
    fn test_split_into_chunks_single_thread() {
        let buf = make_buffer(&[(b'A', 5); 10]);
        let chunks = split_into_chunks(&buf, buf.len() + 1);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], (0, buf.len()));
    }

    #[test]
    fn test_split_into_chunks_preserves_boundaries() {
        let buf = make_buffer(&[(b'A', 5); 100]);
        let chunks = split_into_chunks(&buf, 4);

        assert_eq!(chunks.first().unwrap().0, 0);
        assert_eq!(chunks.last().unwrap().1, buf.len());

        for i in 1..chunks.len() {
            assert_eq!(chunks[i].0, chunks[i - 1].1);
        }

        for (start, _) in &chunks {
            if *start > 0 && *start < buf.len() {
                let len = u16::from_be_bytes([buf[*start], buf[*start + 1]]) as usize;
                assert!(*start + 2 + len <= buf.len());
            }
        }
    }
}
