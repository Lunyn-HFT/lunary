use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::zerocopy::{ZeroCopyMessage, ZeroCopyParser};

pub struct MmapParser {
    mmap: Mmap,
}

const MAX_MMAP_SIZE: u64 = 16 * 1024 * 1024 * 1024;

impl MmapParser {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(&path)?;
        let metadata = file.metadata()?;

        if metadata.len() > MAX_MMAP_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "File too large for mmap: {} bytes (max {})",
                    metadata.len(),
                    MAX_MMAP_SIZE
                ),
            ));
        }

        if metadata.len() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Cannot mmap empty file",
            ));
        }

        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self { mmap })
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.mmap
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    #[inline]
    pub fn parser(&self) -> ZeroCopyParser<'_> {
        ZeroCopyParser::new(&self.mmap)
    }

    pub fn parse_all(&self) -> Vec<ZeroCopyMessage<'_>> {
        let mut parser = self.parser();
        parser.parse_all().collect()
    }

    pub fn into_shared(self) -> MmapParserShared {
        MmapParserShared::from(self)
    }

    pub fn count_messages(&self) -> usize {
        let mut parser = self.parser();
        parser.count()
    }

    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(ZeroCopyMessage<'_>),
    {
        let mut parser = self.parser();
        parser.for_each(f);
    }

    #[cfg(feature = "simd")]
    pub fn prefetch_all(&self) {
        crate::simd::prefetch_range(self.mmap.as_ref());
    }

    #[cfg(not(feature = "simd"))]
    pub fn prefetch_all(&self) {}
}

pub struct ChunkedMmapParser {
    mmap: Mmap,
    chunk_ranges: Vec<(usize, usize)>,
    target_chunk_size: usize,
}

impl ChunkedMmapParser {
    pub fn open<P: AsRef<Path>>(path: P, target_chunk_size: usize) -> io::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let target_chunk_size = target_chunk_size.max(4096);

        let boundaries = Self::build_message_boundaries(&mmap);
        let chunk_ranges = Self::compute_aligned_chunks(&boundaries, mmap.len(), target_chunk_size);

        Ok(Self {
            mmap,
            chunk_ranges,
            target_chunk_size,
        })
    }

    fn build_message_boundaries(data: &[u8]) -> Vec<usize> {
        let estimated = data.len() / 32;
        let mut boundaries = Vec::with_capacity(estimated);
        let mut offset = 0;

        while offset + 2 <= data.len() {
            let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            let next = offset + 2 + len;
            if next > data.len() {
                break;
            }
            boundaries.push(next);
            offset = next;
        }

        boundaries
    }

    fn compute_aligned_chunks(
        boundaries: &[usize],
        total_len: usize,
        target_size: usize,
    ) -> Vec<(usize, usize)> {
        if boundaries.is_empty() {
            return if total_len > 0 {
                vec![(0, total_len)]
            } else {
                Vec::new()
            };
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < total_len {
            let target_end = (start + target_size).min(total_len);

            let end = match boundaries.binary_search(&target_end) {
                Ok(idx) => boundaries[idx],
                Err(idx) => {
                    if idx >= boundaries.len() {
                        *boundaries.last().unwrap_or(&total_len)
                    } else {
                        boundaries[idx]
                    }
                }
            };

            if end > start {
                chunks.push((start, end));
                start = end;
            } else {
                break;
            }
        }

        chunks
    }

    #[inline]
    pub fn chunk_ranges(&self) -> &[(usize, usize)] {
        &self.chunk_ranges
    }

    #[inline]
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.chunk_ranges
            .iter()
            .map(|(start, end)| &self.mmap[*start..*end])
    }

    #[inline]
    pub fn num_chunks(&self) -> usize {
        self.chunk_ranges.len()
    }

    pub fn parse_chunk(&self, chunk_idx: usize) -> Result<(Vec<ZeroCopyMessage<'_>>, usize)> {
        if chunk_idx >= self.chunk_ranges.len() {
            return Ok((Vec::new(), 0));
        }

        let (start, end) = self.chunk_ranges[chunk_idx];
        self.parse_chunk_range(start, end)
    }

    pub fn parse_chunk_range(
        &self,
        start: usize,
        end: usize,
    ) -> Result<(Vec<ZeroCopyMessage<'_>>, usize)> {
        if start >= self.mmap.len() || start >= end {
            return Ok((Vec::new(), 0));
        }

        let chunk = &self.mmap[start..end.min(self.mmap.len())];
        let mut parser = ZeroCopyParser::new(chunk);
        let messages: Vec<_> = parser.parse_all().collect();
        let consumed = parser.position();
        Ok((messages, consumed))
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.mmap
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    #[inline]
    pub fn target_chunk_size(&self) -> usize {
        self.target_chunk_size
    }

    pub fn total_messages(&self) -> usize {
        crate::simd::count_messages_fast(&self.mmap)
    }
}

pub struct MmapParserShared {
    mmap: Arc<Mmap>,
}

impl MmapParserShared {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }

    pub fn data(&self) -> &[u8] {
        self.mmap.as_ref()
    }

    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    pub fn parser(&self) -> ZeroCopyParser<'_> {
        ZeroCopyParser::new(self.mmap.as_ref())
    }

    pub fn parse_all(&self) -> Vec<ZeroCopyMessage<'_>> {
        let mut parser = self.parser();
        parser.parse_all().collect()
    }
}

impl From<MmapParser> for MmapParserShared {
    fn from(p: MmapParser) -> Self {
        Self {
            mmap: Arc::new(p.mmap),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const VALID_TYPES: [u8; 21] = [
        b'S', b'R', b'H', b'Y', b'L', b'V', b'W', b'K', b'A', b'F', b'E', b'C', b'X', b'D', b'U',
        b'P', b'Q', b'B', b'I', b'N', b'J',
    ];

    fn create_test_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0u8; 100]).unwrap();
        file
    }
    fn create_itch_test_file(messages: &[(u8, usize)]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for (msg_type, payload_len) in messages {
            let len = 1 + 10 + payload_len; // type + header + payload
            file.write_all(&(len as u16).to_be_bytes()).unwrap();
            file.write_all(&[*msg_type]).unwrap();
            file.write_all(&[0u8; 10]).unwrap(); // header
            file.write_all(&vec![0xABu8; *payload_len]).unwrap();
        }
        // Ensure all data is synced to disk before mmap reads it
        file.as_file().sync_all().unwrap();
        file
    }

    #[test]
    fn test_mmap_parser_open() {
        let file = create_test_file();
        let parser = MmapParser::open(file.path());
        assert!(parser.is_ok());
    }

    #[test]
    fn test_mmap_parser_len() {
        let file = create_test_file();
        let parser = MmapParser::open(file.path()).unwrap();
        assert_eq!(parser.len(), 100);
    }

    #[test]
    fn test_chunked_parser_message_alignment() {
        let messages: Vec<(u8, usize)> = (0..500)
            .map(|i| (VALID_TYPES[i % VALID_TYPES.len()], (i % 50) + 15))
            .collect();

        let file = create_itch_test_file(&messages);

        let parser = ChunkedMmapParser::open(file.path(), 4096).unwrap();

        assert!(
            parser.num_chunks() > 1,
            "Should have multiple chunks, got {}",
            parser.num_chunks()
        );

        let mut total_messages = 0;
        for chunk_idx in 0..parser.num_chunks() {
            let (msgs, consumed) = parser.parse_chunk(chunk_idx).unwrap();
            total_messages += msgs.len();

            let (start, end) = parser.chunk_ranges()[chunk_idx];
            assert_eq!(
                consumed,
                end - start,
                "Chunk {} should be fully consumed",
                chunk_idx
            );
        }

        assert_eq!(total_messages, 500, "All 500 messages should be parsed");
    }

    #[test]
    fn test_chunked_parser_no_overlap() {
        let messages: Vec<(u8, usize)> = (0..50)
            .map(|_| (b'A', 30)) // Fixed 30-byte payloads, valid 'A' (AddOrder) type
            .collect();

        let file = create_itch_test_file(&messages);
        let parser = ChunkedMmapParser::open(file.path(), 100).unwrap();

        let ranges = parser.chunk_ranges();
        for i in 1..ranges.len() {
            assert_eq!(
                ranges[i].0,
                ranges[i - 1].1,
                "Chunk {} should start where chunk {} ends",
                i,
                i - 1
            );
        }

        if !ranges.is_empty() {
            assert_eq!(ranges[0].0, 0, "First chunk should start at 0");
        }
    }

    #[test]
    fn test_chunked_parser_parallel_safety() {
        let messages: Vec<(u8, usize)> = (0..200)
            .map(|i| (VALID_TYPES[i % VALID_TYPES.len()], 10 + (i % 15)))
            .collect();

        let file = create_itch_test_file(&messages);
        let parser = ChunkedMmapParser::open(file.path(), 500).unwrap();

        let mut all_types: Vec<u8> = Vec::new();
        for chunk_idx in 0..parser.num_chunks() {
            let (msgs, _) = parser.parse_chunk(chunk_idx).unwrap();
            for msg in msgs {
                all_types.push(msg.msg_type());
            }
        }

        assert_eq!(
            all_types.len(),
            200,
            "Should have 200 messages, got {}",
            all_types.len()
        );
        for (i, &msg_type) in all_types.iter().enumerate() {
            let expected = VALID_TYPES[i % VALID_TYPES.len()];
            assert_eq!(msg_type, expected, "Message {} has wrong type", i);
        }
    }
}
