use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

use crate::{
    BatchProcessor, CacheStats, ParallelParser, ParseDiagnosticsExt, Parser, SimdDiagnostics,
    SimdLevel, WorkStealingParser, ZeroCopyParser, compute_checksum_simd, scan_boundaries_auto,
    validate_message_stream_simd,
};

use super::utils::split_into_chunks;

pub fn run_realworld_simulation(data: &[u8]) -> Result<()> {
    println!("=== Real-World Simulation Benchmark ===");
    println!();

    let chunk_sizes = [64 * 1024, 256 * 1024, 1024 * 1024, 4 * 1024 * 1024];

    for chunk_size in chunk_sizes {
        println!("--- Chunk size: {} KB ---", chunk_size / 1024);

        let mut processor = crate::AdaptiveBatchProcessor::with_config(
            crate::AdaptiveBatchConfig::default().with_strategy(crate::AdaptiveStrategy::Balanced),
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

pub fn run_diagnostics(data: &[u8]) -> Result<()> {
    println!("=== Full SIMD/Cache Diagnostics ===");
    println!();

    println!("SIMD Capabilities:");
    super::simd::print_simd_info();
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
    let simd_diag = SimdDiagnostics {
        bytes_processed: data.len() as u64,
        simd_bytes: data.len() as u64,
        scalar_bytes: 0,
        messages_scanned: boundaries.len() as u64,
        prefetch_count: 0,
        level_used: Some(SimdLevel::Avx2),
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

pub fn run_feature_comparison(data: &[u8]) -> Result<()> {
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
    let _ = parser.shutdown();
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
