use lunary::*;

#[test]
fn test_parse_direct_listing_valid() {
    let mut parser = Parser::new();
    let mut buf = vec![];
    buf.extend_from_slice(&35u16.to_be_bytes()); // length
    buf.push(b'O'); // message type
    buf.extend_from_slice(&1u16.to_be_bytes()); // stock_locate
    buf.extend_from_slice(&2u16.to_be_bytes()); // tracking_number
    let ts: u64 = 3;
    buf.extend_from_slice(&ts.to_be_bytes()[2..]); // timestamp (6 bytes)
    buf.extend_from_slice(b"TESTSTOC"); // stock
    buf.extend_from_slice(&4u32.to_be_bytes()); // reference_price
    buf.extend_from_slice(&5u32.to_be_bytes()); // indicative_price
    buf.extend_from_slice(&6u32.to_be_bytes()); // reserve_shares
    buf.extend_from_slice(&7u32.to_be_bytes()); // reserve_price

    parser.feed_data(&buf).unwrap();
    let result = parser.parse_next();
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert!(msg.is_some());
    if let Some(Message::DirectListing(dl)) = msg {
        assert_eq!(dl.stock_locate, 1);
        assert_eq!(dl.tracking_number, 2);
        assert_eq!(dl.timestamp, 3);
        assert_eq!(dl.stock, *b"TESTSTOC");
        assert_eq!(dl.reference_price, 4);
        assert_eq!(dl.indicative_price, 5);
        assert_eq!(dl.reserve_shares, 6);
        assert_eq!(dl.reserve_price, 7);
    } else {
        panic!("Expected DirectListing message");
    }
}

#[test]
fn test_parse_direct_listing_truncated() {
    let mut parser = Parser::new();
    let mut buf = vec![];
    buf.extend_from_slice(&35u16.to_be_bytes());
    buf.push(b'O');
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&2u16.to_be_bytes());
    let ts: u64 = 3;
    buf.extend_from_slice(&ts.to_be_bytes()[2..]);
    buf.extend_from_slice(b"TESTSTOC");
    buf.extend_from_slice(&4u32.to_be_bytes());
    buf.extend_from_slice(&5u32.to_be_bytes());
    buf.extend_from_slice(&6u32.to_be_bytes());
    parser.feed_data(&buf).unwrap();
    let result = parser.parse_next();
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}
