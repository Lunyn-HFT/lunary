use crate::error::ParseError;
use crate::zerocopy_types::MessageHeaderRaw;
use std::marker::PhantomData;
use zerocopy::Ref;

pub trait ParseMessage<'a>: Sized {
    fn parse(data: &'a [u8], msg_type: u8) -> Option<Self>;
    fn msg_type(&self) -> u8;
    fn timestamp(&self) -> u64;
}

pub trait MessageVisitor<'a> {
    fn visit(&mut self, msg: ZeroCopyMessage<'a>);
}

pub trait IntoOwned {
    type Owned;
    fn into_owned(self) -> Self::Owned;
}

#[derive(Debug, Clone, Copy)]
pub struct Stock<'a> {
    data: &'a [u8; 8],
}

impl<'a> Stock<'a> {
    #[inline(always)]
    pub fn new(data: &'a [u8; 8]) -> Self {
        Self { data }
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8; 8] {
        self.data
    }

    #[inline(always)]
    pub fn as_str(&self) -> Result<&str, ParseError> {
        let end = self.data.iter().position(|&b| b == b' ').unwrap_or(8);
        std::str::from_utf8(&self.data[..end])
            .map_err(|_| ParseError::InvalidUtf8 { field: "stock" })
    }

    #[inline(always)]
    pub fn to_owned_bytes(&self) -> [u8; 8] {
        *self.data
    }
}

impl<'a> IntoOwned for Stock<'a> {
    type Owned = [u8; 8];

    #[inline(always)]
    fn into_owned(self) -> Self::Owned {
        *self.data
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mpid<'a> {
    data: &'a [u8; 4],
}

impl<'a> Mpid<'a> {
    #[inline(always)]
    pub fn new(data: &'a [u8; 4]) -> Self {
        Self { data }
    }

    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8; 4] {
        self.data
    }

    #[inline(always)]
    pub fn as_str(&self) -> Result<&str, ParseError> {
        let end = self.data.iter().position(|&b| b == b' ').unwrap_or(4);
        std::str::from_utf8(&self.data[..end])
            .map_err(|_| ParseError::InvalidUtf8 { field: "mpid" })
    }

    #[inline(always)]
    pub fn to_owned_bytes(&self) -> [u8; 4] {
        *self.data
    }
}

impl<'a> IntoOwned for Mpid<'a> {
    type Owned = [u8; 4];

    #[inline(always)]
    fn into_owned(self) -> Self::Owned {
        *self.data
    }
}

pub struct ZeroCopyMessage<'a> {
    header: Ref<&'a [u8], MessageHeaderRaw>,
    payload: &'a [u8],
    msg_type: u8,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> ZeroCopyMessage<'a> {
    #[inline(always)]
    pub fn new(msg_type: u8, header: Ref<&'a [u8], MessageHeaderRaw>, payload: &'a [u8]) -> Self {
        Self {
            header,
            payload,
            msg_type,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn msg_type(&self) -> u8 {
        self.msg_type
    }

    #[inline(always)]
    pub fn stock_locate(&self) -> u16 {
        self.header.stock_locate.get()
    }

    #[inline(always)]
    pub fn tracking_number(&self) -> u16 {
        self.header.tracking_number.get()
    }

    #[inline(always)]
    pub fn timestamp(&self) -> u64 {
        self.header.timestamp()
    }

    #[inline(always)]
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }

    #[inline(always)]
    pub fn read_u8(&self, offset: usize) -> u8 {
        self.payload[offset]
    }

    #[inline(always)]
    pub fn read_u16(&self, offset: usize) -> u16 {
        u16::from_be_bytes([self.payload[offset], self.payload[offset + 1]])
    }

    #[inline(always)]
    pub fn read_u32(&self, offset: usize) -> u32 {
        u32::from_be_bytes([
            self.payload[offset],
            self.payload[offset + 1],
            self.payload[offset + 2],
            self.payload[offset + 3],
        ])
    }

    #[inline(always)]
    pub fn read_u64(&self, offset: usize) -> u64 {
        u64::from_be_bytes([
            self.payload[offset],
            self.payload[offset + 1],
            self.payload[offset + 2],
            self.payload[offset + 3],
            self.payload[offset + 4],
            self.payload[offset + 5],
            self.payload[offset + 6],
            self.payload[offset + 7],
        ])
    }

    #[inline(always)]
    pub fn read_stock(&self, offset: usize) -> &'a [u8] {
        &self.payload[offset..offset + 8]
    }

    #[inline(always)]
    pub fn read_mpid(&self, offset: usize) -> &'a [u8] {
        &self.payload[offset..offset + 4]
    }

    #[inline(always)]
    pub fn read_char(&self, offset: usize) -> char {
        self.payload[offset] as char
    }

    #[inline(always)]
    /// # Safety
    ///
    /// Caller must ensure `offset + 2 <= self.payload.len()`.
    pub unsafe fn read_u16_unchecked(&self, offset: usize) -> u16 {
        let ptr = self.payload.as_ptr().add(offset);
        u16::from_be_bytes(std::ptr::read_unaligned(ptr as *const [u8; 2]))
    }

    #[inline(always)]
    /// # Safety
    ///
    /// Caller must ensure `offset + 4 <= self.payload.len()`.
    pub unsafe fn read_u32_unchecked(&self, offset: usize) -> u32 {
        let ptr = self.payload.as_ptr().add(offset);
        u32::from_be_bytes(std::ptr::read_unaligned(ptr as *const [u8; 4]))
    }

    #[inline(always)]
    /// # Safety
    ///
    /// Caller must ensure `offset + 8 <= self.payload.len()`.
    pub unsafe fn read_u64_unchecked(&self, offset: usize) -> u64 {
        let ptr = self.payload.as_ptr().add(offset);
        u64::from_be_bytes(std::ptr::read_unaligned(ptr as *const [u8; 8]))
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.payload.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
    }
}

impl<'a> ParseMessage<'a> for ZeroCopyMessage<'a> {
    #[inline(always)]
    fn parse(data: &'a [u8], msg_type: u8) -> Option<Self> {
        if data.len() < 10 {
            return None;
        }
        if let Ok((hdr_ref, rest)) = Ref::<&[u8], MessageHeaderRaw>::from_prefix(data) {
            let payload = rest;
            Some(Self::new(msg_type, hdr_ref, payload))
        } else {
            None
        }
    }

    #[inline(always)]
    fn msg_type(&self) -> u8 {
        self.msg_type
    }

    #[inline(always)]
    fn timestamp(&self) -> u64 {
        self.header.timestamp()
    }
}

pub struct ZeroCopyParser<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ZeroCopyParser<'a> {
    #[inline(always)]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    #[inline(always)]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    #[inline(always)]
    pub fn position(&self) -> usize {
        self.position
    }

    #[inline(always)]
    pub fn parse_next(&mut self) -> Option<ZeroCopyMessage<'a>> {
        if self.remaining() < 3 {
            return None;
        }

        let pos = self.position;
        let length = u16::from_be_bytes([self.data[pos], self.data[pos + 1]]) as usize;
        let total_size = length + 2;

        if self.remaining() < total_size {
            return None;
        }

        let msg_type = self.data[pos + 2];
        let header_start = pos + 3;
        let header_end = header_start + 10;
        let payload_start = header_end;
        let payload_end = pos + total_size;

        if payload_end > self.data.len() || header_end > self.data.len() {
            return None;
        }

        if let Ok((hdr_ref, _)) =
            Ref::<&[u8], MessageHeaderRaw>::from_prefix(&self.data[header_start..header_end])
        {
            let payload = &self.data[payload_start..payload_end];
            self.position = pos + total_size;
            return Some(ZeroCopyMessage::new(msg_type, hdr_ref, payload));
        }

        None
    }

    #[inline]
    pub fn parse_all(&mut self) -> Vec<ZeroCopyMessage<'a>> {
        let estimated = self.remaining() / 32;
        let mut messages = Vec::with_capacity(estimated);
        while let Some(msg) = self.parse_next() {
            messages.push(msg);
        }
        messages
    }

    #[inline]
    pub fn for_each<F>(&mut self, mut f: F)
    where
        F: FnMut(ZeroCopyMessage<'a>),
    {
        while let Some(msg) = self.parse_next() {
            f(msg);
        }
    }

    #[inline]
    pub fn count(&mut self) -> usize {
        let mut count = 0;
        while self.parse_next().is_some() {
            count += 1;
        }
        count
    }

    #[inline]
    pub fn reset(&mut self) {
        self.position = 0;
    }

    #[inline(always)]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    #[inline]
    pub fn skip(&mut self, n: usize) {
        for _ in 0..n {
            if self.parse_next().is_none() {
                break;
            }
        }
    }

    #[inline]
    pub fn take(&mut self, n: usize) -> Vec<ZeroCopyMessage<'a>> {
        let mut messages = Vec::with_capacity(n);
        for _ in 0..n {
            match self.parse_next() {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }
        messages
    }
}

pub struct ZeroCopyIterator<'a> {
    parser: ZeroCopyParser<'a>,
}

impl<'a> Iterator for ZeroCopyIterator<'a> {
    type Item = ZeroCopyMessage<'a>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.parser.parse_next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.parser.remaining();
        let min = remaining / 64;
        let max = remaining / 3;
        (min, Some(max))
    }
}

impl<'a> IntoIterator for ZeroCopyParser<'a> {
    type Item = ZeroCopyMessage<'a>;
    type IntoIter = ZeroCopyIterator<'a>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        ZeroCopyIterator { parser: self }
    }
}

pub struct ZeroCopyBatchProcessor<'a> {
    data: &'a [u8],
    position: usize,
    batch_size: usize,
    messages_processed: u64,
    bytes_processed: u64,
}

impl<'a> ZeroCopyBatchProcessor<'a> {
    pub fn new(data: &'a [u8], batch_size: usize) -> Self {
        Self {
            data,
            position: 0,
            batch_size: batch_size.max(1),
            messages_processed: 0,
            bytes_processed: 0,
        }
    }

    #[inline]
    pub fn process_batch(&mut self) -> Vec<ZeroCopyMessage<'a>> {
        let mut messages = Vec::with_capacity(self.batch_size);
        let mut count = 0;

        while count < self.batch_size && self.position + 3 <= self.data.len() {
            let pos = self.position;

            let msg_len = u16::from_be_bytes([self.data[pos], self.data[pos + 1]]) as usize;
            let total_size = msg_len + 2;

            if pos + total_size > self.data.len() {
                break;
            }

            if msg_len < 11 {
                self.position = pos + total_size;
                continue;
            }

            let msg_type = self.data[pos + 2];
            let header_start = pos + 3;
            let header_end = header_start + 10;
            let payload_start = header_end;
            let payload_end = pos + total_size;

            if header_end > self.data.len() || payload_end > self.data.len() {
                break;
            }

            if let Ok((hdr_ref, _)) =
                Ref::<&[u8], MessageHeaderRaw>::from_prefix(&self.data[header_start..header_end])
            {
                let payload = &self.data[payload_start..payload_end];
                messages.push(ZeroCopyMessage::new(msg_type, hdr_ref, payload));
                self.position = pos + total_size;
                self.bytes_processed += total_size as u64;
                count += 1;
            }
        }

        self.messages_processed += messages.len() as u64;
        messages
    }

    #[inline]
    pub fn process_all(&mut self) -> Vec<ZeroCopyMessage<'a>> {
        let estimated = (self.data.len() - self.position) / 32;
        let mut all_messages = Vec::with_capacity(estimated);

        loop {
            let batch = self.process_batch();
            if batch.is_empty() {
                break;
            }
            all_messages.extend(batch);
        }

        all_messages
    }

    #[inline]
    pub fn for_each<F>(&mut self, mut f: F)
    where
        F: FnMut(ZeroCopyMessage<'a>),
    {
        loop {
            let batch = self.process_batch();
            if batch.is_empty() {
                break;
            }
            for msg in batch {
                f(msg);
            }
        }
    }

    #[inline]
    pub fn for_each_batch<F>(&mut self, mut f: F)
    where
        F: FnMut(&[ZeroCopyMessage<'a>]),
    {
        loop {
            let batch = self.process_batch();
            if batch.is_empty() {
                break;
            }
            f(&batch);
        }
    }

    #[inline]
    pub fn messages_processed(&self) -> u64 {
        self.messages_processed
    }

    #[inline]
    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    #[inline]
    pub fn is_done(&self) -> bool {
        self.position + 3 > self.data.len()
    }

    #[inline]
    pub fn reset(&mut self) {
        self.position = 0;
        self.messages_processed = 0;
        self.bytes_processed = 0;
    }

    #[inline]
    pub fn set_batch_size(&mut self, size: usize) {
        self.batch_size = size.max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stock_as_str() {
        let data: [u8; 8] = *b"AAPL    ";
        let stock = Stock::new(&data);
        assert_eq!(stock.as_str().unwrap(), "AAPL");
    }

    #[test]
    fn test_mpid_as_str() {
        let data: [u8; 4] = *b"NSDQ";
        let mpid = Mpid::new(&data);
        assert_eq!(mpid.as_str().unwrap(), "NSDQ");
    }
}
