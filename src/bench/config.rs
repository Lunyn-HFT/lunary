use std::path::PathBuf;

use crate::Config;
use clap::{Arg, Command};

#[derive(Clone)]
pub struct BenchConfig {
    pub iterations: usize,
    pub warmup_iterations: usize,
    pub output_format: OutputFormat,
    pub validate: bool,
    pub max_buffer_size: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub enum OutputFormat {
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
            max_buffer_size: 512 * 1024 * 1024,
        }
    }
}

pub struct CliArgs {
    pub path: PathBuf,
    pub mode: String,
    pub config: BenchConfig,
}

pub fn parse_cli_args() -> Option<CliArgs> {
    let matches = Command::new("itch-bench")
        .version(env!("CARGO_PKG_VERSION"))
        .about("High-performance ITCH parser for NASDAQ data")
        .arg(
            Arg::new("file")
                .help("Path to the ITCH file to parse")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("mode")
                .help("Parsing mode")
                .long_help(
                    "Basic Modes:\n\
                     simple - Basic single-threaded parsing\n\
                     batch - Batch processing\n\
                     adaptive - Adaptive batching\n\
                     parallel - Parallel processing\n\
                     worksteal - Work-stealing parallel\n\
                     mmap - Memory-mapped parsing\n\
                     decode - Message decoding\n\
                     zerocopy - Zero-copy parsing\n\
                     spsc - Single-producer single-consumer\n\
                     simd - SIMD-accelerated parsing\n\
                     \n\
                     Advanced Modes:\n\
                     zerocopy-ref - Zero-copy reference parsing with MessageRef\n\
                     adaptive-all - Compare all adaptive strategies\n\
                     worker-stats - Parallel parsing with per-worker stats\n\
                     simd-validate - SIMD message validation benchmark\n\
                     latency - Latency distribution analysis\n\
                     realworld - Real-world simulation benchmark\n\
                     diagnostics - Full SIMD/cache diagnostics\n\
                     feature-cmp - Per-feature comparison (zerocopy vs owned, SIMD vs scalar)\n\
                     fuzzing - Fuzzing and error injection test\n\
                     all - Run all benchmarks",
                )
                .value_parser([
                    "simple",
                    "batch",
                    "adaptive",
                    "parallel",
                    "worksteal",
                    "mmap",
                    "decode",
                    "zerocopy",
                    "spsc",
                    "simd",
                    "zerocopy-ref",
                    "adaptive-all",
                    "worker-stats",
                    "simd-validate",
                    "latency",
                    "realworld",
                    "diagnostics",
                    "feature-cmp",
                    "fuzzing",
                    "all",
                ])
                .default_value("all")
                .index(2),
        )
        .arg(
            Arg::new("max_size_mb")
                .help("Maximum file/buffer size in MB")
                .value_parser(clap::value_parser!(usize))
                .default_value("512")
                .index(3),
        )
        .arg(
            Arg::new("iterations")
                .help("Run each benchmark N times")
                .long("iterations")
                .short('i')
                .value_parser(clap::value_parser!(usize))
                .default_value("1"),
        )
        .arg(
            Arg::new("warmup")
                .help("Run N warmup iterations before measurement")
                .long("warmup")
                .short('w')
                .value_parser(clap::value_parser!(usize))
                .default_value("0"),
        )
        .arg(
            Arg::new("json")
                .help("Output results in JSON format")
                .long("json")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("csv")
                .help("Output results in CSV format")
                .long("csv")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_validate")
                .help("Disable validation")
                .long("no-validate")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let path = PathBuf::from(matches.get_one::<String>("file").unwrap());
    let mode = matches.get_one::<String>("mode").unwrap().clone();
    let max_size_mb = *matches.get_one::<usize>("max_size_mb").unwrap();
    let iterations = *matches.get_one::<usize>("iterations").unwrap();
    let warmup_iterations = *matches.get_one::<usize>("warmup").unwrap();
    let output_format = if matches.get_flag("json") {
        OutputFormat::Json
    } else if matches.get_flag("csv") {
        OutputFormat::Csv
    } else {
        OutputFormat::Human
    };
    let validate = !matches.get_flag("no_validate");

    let lunary_config = Config::from_size_mb(max_size_mb);

    let bench_config = BenchConfig {
        iterations,
        warmup_iterations,
        output_format,
        validate,
        max_buffer_size: lunary_config.max_buffer_size,
    };

    Some(CliArgs {
        path,
        mode,
        config: bench_config,
    })
}

pub fn print_config_summary(data_len: usize, max_size_mb: usize, mmap_len: usize) {
    println!("=== Configuration ===");
    println!("  Max buffer size: {} MB", max_size_mb);
    println!("  File size: {:.2} MB", mmap_len as f64 / (1024.0 * 1024.0));
    println!(
        "  Data loaded: {:.2} MB",
        data_len as f64 / (1024.0 * 1024.0)
    );
    println!();
}
