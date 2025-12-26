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
    chunk_size: usize,
}

impl ChunkedMmapParser {
    pub fn open<P: AsRef<Path>>(path: P, chunk_size: usize) -> io::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap,
            chunk_size: chunk_size.max(4096),
        })
    }

    #[inline]
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.mmap.chunks(self.chunk_size)
    }

    #[inline]
    pub fn num_chunks(&self) -> usize {
        self.mmap.len().div_ceil(self.chunk_size)
    }

    pub fn parse_chunk(&self, chunk_idx: usize) -> Result<(Vec<ZeroCopyMessage<'_>>, usize)> {
        let start = chunk_idx * self.chunk_size;
        if start >= self.mmap.len() {
            return Ok((Vec::new(), 0));
        }
        let end = (start + self.chunk_size).min(self.mmap.len());
        let chunk = &self.mmap[start..end];
        let mut parser = ZeroCopyParser::new(chunk);
        let messages: Vec<_> = parser.parse_all().collect();
        let consumed = parser.position();
        Ok((messages, consumed))
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

    fn create_test_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0u8; 100]).unwrap();
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
}
