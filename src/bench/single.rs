use anyhow::Result;
use std::io::Write;
use std::time::Instant;

use crate::{BatchProcessor, Message, Parser, ZeroCopyParser};

use super::utils::calculate_throughput;
use super::{BenchConfig, OutputFormat};

pub fn bench_simple(data: &[u8]) -> Result<(u64, f64, f64)> {
    let mut parser = Parser::default();
    let t0 = Instant::now();
    let stats = parser.parse(data)?;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(stats.messages, wall);
    Ok((stats.messages, elapsed_ms, mps))
}

pub fn bench_batch(data: &[u8], batch_size: usize) -> Result<(u64, f64, f64)> {
    let mut processor = BatchProcessor::new(batch_size);
    let t0 = Instant::now();
    let messages = processor.process_all(data)?;
    let wall = t0.elapsed();
    let (elapsed_ms, mps) = calculate_throughput(messages.len() as u64, wall);
    Ok((messages.len() as u64, elapsed_ms, mps))
}

pub fn bench_zerocopy(data: &[u8]) -> Result<(u64, f64, f64)> {
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

pub fn bench_zerocopy_ref(data: &[u8]) -> Result<(u64, f64, f64)> {
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

pub fn run_simple(data: &[u8], config: &BenchConfig) -> Result<()> {
    let (messages, elapsed_ms, mps) = bench_simple(data)?;
    if config.output_format != OutputFormat::Human {
        let filename = match config.output_format {
            OutputFormat::Json => "results.json",
            OutputFormat::Csv => "results.csv",
            _ => unreachable!(),
        };
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(filename)?;
        match config.output_format {
            OutputFormat::Json => {
                writeln!(file, "{{")?;
                writeln!(file, "  \"name\": \"simple\",")?;
                writeln!(file, "  \"messages\": {},", messages)?;
                writeln!(file, "  \"time_ms\": {:.2},", elapsed_ms)?;
                writeln!(file, "  \"mps\": {:.2}", mps)?;
                writeln!(file, "}}")?;
            }
            OutputFormat::Csv => {
                if file.metadata()?.len() == 0 {
                    writeln!(file, "name,messages,time_ms,mps,variance")?;
                }
                writeln!(file, "simple,{},{:.2},{:.2},0.0", messages, elapsed_ms, mps)?;
            }
            _ => {}
        }
    } else {
        println!(
            "[simple] Parsed {} messages ({} bytes) in {:.2}ms => {:.2} M msg/sec",
            messages,
            data.len(),
            elapsed_ms,
            mps
        );
    }
    Ok(())
}

pub fn run_batch(data: &[u8]) -> Result<()> {
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

pub fn run_mmap(data: &[u8]) -> Result<()> {
    let t0 = Instant::now();
    let mut parser = ZeroCopyParser::new(data);
    let messages: Vec<_> = parser.parse_all().collect();
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

pub fn run_decode(data: &[u8]) -> Result<()> {
    let mut parser = Parser::default();
    let t0 = Instant::now();
    let messages = parser.parse_all(data)?.collect::<Result<Vec<_>, _>>()?;
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
        let type_name = get_message_type_name(msg);
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

pub fn run_zerocopy(data: &[u8]) -> Result<()> {
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

pub fn run_zerocopy_ref(data: &[u8]) -> Result<()> {
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

fn get_message_type_name(msg: &Message) -> &'static str {
    match msg {
        Message::SystemEvent(_) => "SystemEvent",
        Message::StockDirectory(_) => "StockDirectory",
        Message::StockTradingAction(_) => "StockTradingAction",
        Message::RegShoRestriction(_) => "RegShoRestriction",
        Message::MarketParticipantPosition(_) => "MarketParticipantPosition",
        Message::MwcbDeclineLevel(_) => "MwcbDeclineLevel",
        Message::MwcbStatus(_) => "MwcbStatus",
        Message::IpoQuotingPeriod(_) => "IpoQuotingPeriod",
        Message::AddOrder(_) => "AddOrder",
        Message::AddOrderWithMpid(_) => "AddOrderWithMpid",
        Message::OrderExecuted(_) => "OrderExecuted",
        Message::OrderExecutedWithPrice(_) => "OrderExecutedWithPrice",
        Message::OrderCancel(_) => "OrderCancel",
        Message::OrderDelete(_) => "OrderDelete",
        Message::OrderReplace(_) => "OrderReplace",
        Message::Trade(_) => "Trade",
        Message::CrossTrade(_) => "CrossTrade",
        Message::BrokenTrade(_) => "BrokenTrade",
        Message::NetOrderImbalance(_) => "NetOrderImbalance",
        Message::RetailPriceImprovement(_) => "RetailPriceImprovement",
        Message::LuldAuctionCollar(_) => "LuldAuctionCollar",
        Message::DirectListing(_) => "DirectListing",
    }
}
