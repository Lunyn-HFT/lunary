#![no_main]

use libfuzzer_sys::fuzz_target;
use lunary::{BatchProcessor, ZeroCopyParser};
#[cfg(feature = "simd")]
use lunary::validate_message_stream_simd;

const ITERATION_MULTIPLIER: usize = 4;
const ITERATION_CAP_MAX: usize = 100_000;
const SIMD_DIVISOR: usize = 8;
const SIMD_LIMIT_MAX: usize = 1_000;

fuzz_target!(|data: &[u8]| {
    let data_start = data.as_ptr() as usize;
    let data_end = data_start + data.len();

    let cap = (data.len().max(1) * ITERATION_MULTIPLIER).min(ITERATION_CAP_MAX);
    let simd_limit = (data.len().max(1) / SIMD_DIVISOR).min(SIMD_LIMIT_MAX);

    {
        let mut parser = ZeroCopyParser::new(data);
        let mut prev_pos = parser.position();
        let mut iterations = 0usize;
        loop {
            if iterations > cap {
                panic!("zero_copy_parser: iteration cap exceeded");
            }
            iterations += 1;
            match parser.parse_next() {
                Some(msg) => {
                    let curr_pos = parser.position();
                    if curr_pos < prev_pos {
                        panic!("zero_copy_parser: position decreased");
                    }
                    if curr_pos > data.len() {
                        panic!("zero_copy_parser: position exceeded input length");
                    }
                    if curr_pos == prev_pos {
                        panic!("zero_copy_parser: no forward progress");
                    }

                    let payload = msg.payload();
                    let p_start = payload.as_ptr() as usize;
                    let p_end = p_start.checked_add(payload.len()).expect("payload pointer overflow");
                    if p_start < data_start || p_end > data_end {
                        panic!("zero_copy_parser: payload outside original input");
                    }

                    let _ = msg.stock_locate();
                    let _ = msg.tracking_number();
                    let _ = msg.timestamp();

                    prev_pos = curr_pos;
                }
                None => break,
            }
        }
    }

    #[cfg(feature = "simd")]
    {
        let validation = validate_message_stream_simd(data, simd_limit);
        if validation.is_valid() {
            let mut bp = BatchProcessor::new(1024);
            let res = bp.process_all(data);
            match res {
                Ok(msgs) => {
                    let mut zp = ZeroCopyParser::new(data);
                    let mut count = 0usize;
                    while zp.parse_next().is_some() {
                        count += 1;
                        if count > cap {
                            break;
                        }
                    }
                    if count != msgs.len() {
                        panic!(
                            "parser mismatch: zero_copy_count={} batch_count={}"
                            , count, msgs.len()
                        );
                    }
                }
                Err(e) => panic!("simd indicated valid but BatchProcessor returned Err: {:?}", e),
            }
        }
    }

    #[cfg(not(feature = "simd"))]
    {
        let mut bp = BatchProcessor::new(1024);
        let _ = bp.process_all(data);
    }
});