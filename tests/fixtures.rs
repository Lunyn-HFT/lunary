pub fn create_test_buffer(messages: &[(u8, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (msg_type, payload) in messages {
        let len = 1 + payload.len();
        buf.extend_from_slice(&(len as u16).to_be_bytes());
        buf.push(*msg_type);
        buf.extend_from_slice(payload);
    }
    buf
}

#[allow(dead_code)]
pub fn create_uniform_buffer(count: usize, msg_type: u8, payload_len: usize) -> Vec<u8> {
    let payload = vec![0xAB; payload_len];
    let messages: Vec<(u8, &[u8])> = (0..count).map(|_| (msg_type, payload.as_slice())).collect();
    create_test_buffer(&messages)
}

#[allow(dead_code)]
pub fn standard_fixture() -> Vec<u8> {
    let mut messages = Vec::new();
    for i in 0..1000u16 {
        let msg_type = b'A' + (i % 26) as u8;
        let payload: Vec<u8> = (0..(10 + (i % 20) as usize)).map(|j| j as u8).collect();
        messages.push((msg_type, payload));
    }
    let mut buf = Vec::new();
    for (msg_type, payload) in &messages {
        let len = 1 + payload.len();
        buf.extend_from_slice(&(len as u16).to_be_bytes());
        buf.push(*msg_type);
        buf.extend_from_slice(payload);
    }
    buf
}
