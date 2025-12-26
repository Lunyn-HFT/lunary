mod fixtures;

use lunary::ZeroCopyParser;

use fixtures::create_test_buffer;

#[test]
fn test_skip_invalid_message_type() {
    let messages = vec![
        (0xFF, &[][..]),      // invalid
        (b'S', &[0; 10][..]), // valid
    ];
    let buf = create_test_buffer(&messages);
    let mut parser = ZeroCopyParser::new(&buf);
    let msg = parser.parse_next().unwrap();
    assert_eq!(msg.msg_type(), b'S');
    assert_eq!(parser.position(), buf.len());
    assert!(parser.parse_next().is_none());
}

#[test]
fn test_skip_invalid_with_while_loop() {
    let messages = vec![(0xFF, &[][..]), (b'S', &[0; 10][..]), (b'R', &[1; 5][..])];
    let buf = create_test_buffer(&messages);
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
