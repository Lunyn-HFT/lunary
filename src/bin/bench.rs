use anyhow::Result;
use memmap2::MmapOptions;
use std::fs::File;

use lunary::bench::{
    config::{parse_cli_args, print_config_summary},
    run_mode,
    simd::print_simd_info,
};

fn main() -> Result<()> {
    let cli_args = parse_cli_args().unwrap();

    let path = cli_args.path;
    let mode = cli_args.mode;
    let bench_config = cli_args.config;

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

    let max_size = bench_config.max_buffer_size;
    let data = &mmap[..std::cmp::min(mmap.len(), max_size)];

    let max_size_mb = max_size / (1024 * 1024);
    print_config_summary(data.len(), max_size_mb, mmap.len());

    run_mode(&mode, &path, data, &bench_config)
}
