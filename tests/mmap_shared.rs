use std::io::Write;

use lunary::mmap::MmapParser;

#[test]
fn test_mmap_shared_and_owned_messages() {
    let mut file = tempfile::NamedTempFile::new().unwrap();

    let mut buf = Vec::new();
    for _ in 0..2 {
        buf.extend(&[0, 11]);
        buf.push(b'S');
        buf.extend(&[0u8; 10]);
    }

    file.write_all(&buf).unwrap();

    let parser = MmapParser::open(file.path()).unwrap();
    let shared = parser.into_shared();

    let mut p = shared.parser();
    let owned = p.parse_all_owned();
    assert_eq!(owned.len(), 2);

    drop(shared);
    assert_eq!(owned[0].msg_type, b'S');
}
