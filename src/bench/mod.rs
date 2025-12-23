pub mod adaptive;
pub mod config;
pub mod diagnostics;
pub mod fuzz;
pub mod parallel;
pub mod simd;
pub mod single;
pub mod utils;

#[cfg(test)]
pub mod tests;

pub use config::{BenchConfig, OutputFormat};
pub use utils::{calculate_throughput, find_message_boundary, split_into_chunks};

use anyhow::Result;
use std::io::Write;
use std::path::PathBuf;

use crate::AdaptiveBatchConfig;

pub fn run_mode(mode: &str, path: &PathBuf, data: &[u8], bench_config: &BenchConfig) -> Result<()> {
    match mode {
        "simple" => single::run_simple(data, bench_config),
        "batch" => single::run_batch(data),
        "adaptive" => adaptive::run_adaptive(data),
        "parallel" => parallel::run_parallel(data),
        "worksteal" => parallel::run_worksteal(data),
        "mmap" => single::run_mmap(data),
        "decode" => single::run_decode(data),
        "zerocopy" => single::run_zerocopy(data),
        "zerocopy-ref" => single::run_zerocopy_ref(data),
        "spsc" => parallel::run_spsc(data),
        "simd" => simd::run_simd_bench(data),
        "simd-validate" => simd::run_simd_validate(data),
        "adaptive-all" => adaptive::run_adaptive_strategies(data),
        "worker-stats" => parallel::run_worker_stats(data),
        "latency" => adaptive::run_latency_analysis(data),
        "realworld" => diagnostics::run_realworld_simulation(data),
        "diagnostics" => diagnostics::run_diagnostics(data),
        "feature-cmp" => diagnostics::run_feature_comparison(data),
        "fuzzing" => fuzz::run_fuzzing_test(data),
        "all" => run_all_benchmarks(path, data, bench_config),
        _ => {
            eprintln!("Unknown mode: {}", mode);
            std::process::exit(1);
        }
    }
}

pub fn run_all_benchmarks(_path: &PathBuf, data: &[u8], config: &BenchConfig) -> Result<()> {
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
        ("Simple", Box::new(single::bench_simple)),
        ("Batch (4096)", Box::new(|d| single::bench_batch(d, 4096))),
        ("Batch (8192)", Box::new(|d| single::bench_batch(d, 8192))),
        ("Adaptive", Box::new(adaptive::bench_adaptive)),
        (
            "Adaptive-Low",
            Box::new(|d| adaptive::bench_adaptive_config(d, AdaptiveBatchConfig::low_latency())),
        ),
        (
            "Adaptive-High",
            Box::new(|d| {
                adaptive::bench_adaptive_config(d, AdaptiveBatchConfig::high_throughput())
            }),
        ),
        ("Parallel", Box::new(parallel::bench_parallel)),
        ("WorkSteal", Box::new(parallel::bench_worksteal)),
        ("SPSC", Box::new(parallel::bench_spsc)),
        ("ZeroCopy", Box::new(single::bench_zerocopy)),
        ("ZeroCopy-Ref", Box::new(single::bench_zerocopy_ref)),
        ("SIMD Scan", Box::new(simd::bench_simd_scan)),
        ("SIMD Validate", Box::new(simd::bench_simd_validate)),
    ];

    for (idx, (name, bench_fn)) in bench_fns.into_iter().enumerate() {
        eprint!("[{}/13] {} ", idx + 1, name);
        let _ = std::io::stderr().flush();

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

            let median_time = utils::median_f64(&mut times);
            let median_mps = utils::median_f64(&mut mps_values);
            let variance = utils::variance_f64(&mps_values);

            results.push((name, message_count, median_time, median_mps, variance));
        } else {
            let (msgs, elapsed_ms, mps) = bench_fn(data)?;
            results.push((name, msgs, elapsed_ms, mps, 0.0));
        }
        eprintln!("✓");
    }

    print_results(&results, config, data.len() as f64 / (1024.0 * 1024.0))?;

    Ok(())
}

fn print_results(
    results: &[(&str, u64, f64, f64, f64)],
    config: &BenchConfig,
    file_size_mb: f64,
) -> std::io::Result<()> {
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

            for (name, messages, elapsed_ms, mps, variance) in results {
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
                        println!("\n Warning: Message count mismatches detected!");
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
            Ok(())
        }
        OutputFormat::Json => {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("results.json")?;
            writeln!(file, "{{")?;
            writeln!(file, "  \"file_size_mb\": {:.2},", file_size_mb)?;
            writeln!(file, "  \"iterations\": {},", config.iterations)?;
            writeln!(
                file,
                "  \"warmup_iterations\": {},",
                config.warmup_iterations
            )?;
            writeln!(file, "  \"results\": [")?;
            for (i, (name, messages, elapsed_ms, mps, variance)) in results.iter().enumerate() {
                let comma = if i < results.len() - 1 { "," } else { "" };
                writeln!(
                    file,
                    "    {{\"name\": \"{}\", \"messages\": {}, \"time_ms\": {:.2}, \"mps\": {:.2}, \"variance\": {:.2}}}{}",
                    name, messages, elapsed_ms, mps, variance, comma
                )?;
            }
            writeln!(file, "  ]")?;
            writeln!(file, "}}")?;
            Ok(())
        }
        OutputFormat::Csv => {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open("results.csv")?;
            if file.metadata()?.len() == 0 {
                writeln!(file, "name,messages,time_ms,mps,variance")?;
            }
            for (name, messages, elapsed_ms, mps, variance) in results {
                writeln!(
                    file,
                    "{},{},{:.2},{:.2},{:.2}",
                    name, messages, elapsed_ms, mps, variance
                )?;
            }
            Ok(())
        }
    }
}
