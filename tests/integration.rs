mod fixtures;

use lunary::{BatchProcessor, Parser, ZeroCopyParser};

#[test]
fn test_message_boundary_detection() {
    let buf = fixtures::create_test_buffer(&[
        (b'A', &[1, 2, 3, 4, 5]),
        (b'B', &[10, 20]),
        (b'C', &[100]),
    ]);

    let mut boundaries = vec![0usize];
    let mut pos = 0;
    while pos + 2 <= buf.len() {
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        if pos + 2 + len > buf.len() {
            break;
        }
        pos += 2 + len;
        boundaries.push(pos);
    }

    assert_eq!(boundaries, vec![0, 8, 13, 17]);
}

#[test]
fn test_parser_count_consistency() {
    let buf = fixtures::standard_fixture();

    let mut expected_count = 0;
    let mut pos = 0;
    while pos + 2 <= buf.len() {
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        if pos + 2 + len > buf.len() {
            break;
        }
        pos += 2 + len;
        expected_count += 1;
    }

    assert_eq!(expected_count, 1000);
}

#[test]
fn test_invalid_utf8_stock() {
    let mut parser = Parser::new();
    let mut buf = vec![];
    buf.extend_from_slice(&39u16.to_be_bytes());
    buf.push(b'R');
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    buf.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC, 0x20, 0x20, 0x20, 0x20]);
    buf.push(b'Q');
    buf.push(b'N');
    buf.extend_from_slice(&1000u32.to_be_bytes());
    buf.push(b'Y');
    buf.push(b'A');
    buf.extend_from_slice(b"NY");
    buf.push(b' ');
    buf.push(b' ');
    buf.push(b' ');
    buf.push(b' ');
    buf.push(b' ');
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.push(b' ');

    parser.feed_data(&buf).unwrap();
    let result = parser.parse_next();
    assert!(result.is_ok());
}

#[test]
fn test_invalid_utf8_mpid() {
    let mut parser = Parser::new();
    let mut buf = vec![];
    buf.extend_from_slice(&26u16.to_be_bytes());
    buf.push(b'L');
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    buf.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);
    buf.extend_from_slice(&[b'A', b'A', b'A', b'A', 0x20, 0x20, 0x20, 0x20]);
    buf.push(b'Y');
    buf.push(b' ');
    buf.push(b' ');

    parser.feed_data(&buf).unwrap();
    let result = parser.parse_next();
    assert!(result.is_ok());
}

#[test]
fn test_truncated_data_parsing() {
    let buf = fixtures::standard_fixture();
    for truncate_pct in [10, 50, 90] {
        let truncate_at = buf.len() * truncate_pct / 100;
        let truncated = &buf[..truncate_at];

        let result = std::panic::catch_unwind(|| {
            let mut parser = ZeroCopyParser::new(truncated);
            parser.count()
        });
        assert!(
            result.is_ok(),
            "ZeroCopyParser panicked on {}% truncated data",
            truncate_pct
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut processor = BatchProcessor::new(1024);
            processor.process_all(truncated)
        }));
        assert!(
            result.is_ok(),
            "BatchProcessor panicked on {}% truncated data",
            truncate_pct
        );
    }
}

#[test]
fn test_corrupted_message_types() {
    let mut buf = fixtures::standard_fixture();
    // Corrupt every 10th message type
    let mut offset = 0;
    let mut message_index = 0;
    while offset + 3 < buf.len() && message_index < 100 {
        let len = u16::from_be_bytes([buf[offset], buf[offset + 1]]) as usize;
        if len == 0 || offset + 2 + len > buf.len() {
            break;
        }
        if message_index % 10 == 0 {
            buf[offset + 2] = 0xFF; // Invalid message type
        }
        message_index += 1;
        offset += 2 + len;
    }

    let result = std::panic::catch_unwind(|| {
        let mut parser = ZeroCopyParser::new(&buf);
        parser.count()
    });
    assert!(
        result.is_ok(),
        "ZeroCopyParser panicked on corrupted message types"
    );

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut processor = BatchProcessor::new(1024);
        processor.process_all(&buf)
    }));
    assert!(
        result.is_ok(),
        "BatchProcessor panicked on corrupted message types"
    );
}

#[test]
fn test_large_buffer_no_data_loss() {
    use lunary::Config;

    let config = Config::new().with_max_buffer_size(100 * 1024); // 100 KB
    let mut parser = Parser::with_config(config);

    let mut total_messages = 0;
    let mut data = vec![];

    for _ in 0..50 {
        let payload = vec![b'X'; 35];
        let len = 36u16; // 1 + 35
        data.extend_from_slice(&len.to_be_bytes());
        data.push(b'A');
        data.extend_from_slice(&payload);
        total_messages += 1;
    }

    let chunk_size = 10 * 1024;
    let mut pos = 0;
    while pos < data.len() {
        let end = (pos + chunk_size).min(data.len());
        parser.feed_data(&data[pos..end]).unwrap();
        pos = end;

        while parser.parse_next().unwrap().is_some() {
            total_messages -= 1;
        }
    }

    while parser.parse_next().unwrap().is_some() {
        total_messages -= 1;
    }

    assert_eq!(
        total_messages, 0,
        "All messages should be parsed without data loss"
    );
}

#[test]
fn test_skip_invalid_message_type() {
    let messages = vec![
        (0xFF, &[][..]),      // invalid
        (b'S', &[0; 10][..]), // valid
    ];
    let buf = fixtures::create_test_buffer(&messages);
    let mut parser = ZeroCopyParser::new(&buf);
    let msg = parser.parse_next().unwrap();
    assert_eq!(msg.msg_type(), b'S');
    assert_eq!(parser.position(), buf.len());
    assert!(parser.parse_next().is_none());
}

#[test]
fn test_skip_invalid_with_while_loop() {
    let messages = vec![(0xFF, &[][..]), (b'S', &[0; 10][..]), (b'R', &[1; 5][..])];
    let buf = fixtures::create_test_buffer(&messages);
    let mut parser = ZeroCopyParser::new(&buf);
    let mut count = 0;
    while let Some(msg) = parser.parse_next() {
        count += 1;
        if count == 1 {
            assert_eq!(msg.msg_type(), b'S');
        } else {
            panic!("Unexpected extra message with type {}", msg.msg_type());
        }
    }
    assert_eq!(count, 1);
    assert_eq!(parser.position(), buf.len());
}
