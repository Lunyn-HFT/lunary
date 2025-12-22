use zerocopy::byteorder::network_endian::U16;

#[derive(zerocopy::FromBytes, zerocopy::Unaligned, zerocopy::Immutable, zerocopy::KnownLayout)]
#[repr(C)]
pub struct MessageHeaderRaw {
    pub stock_locate: U16,
    pub tracking_number: U16,
    pub timestamp6: [u8; 6],
}

impl MessageHeaderRaw {
    pub fn stock_locate(&self) -> u16 {
        self.stock_locate.get()
    }

    pub fn tracking_number(&self) -> u16 {
        self.tracking_number.get()
    }

    pub fn timestamp(&self) -> u64 {
        let b = &self.timestamp6;
        ((b[0] as u64) << 40)
            | ((b[1] as u64) << 32)
            | ((b[2] as u64) << 24)
            | ((b[3] as u64) << 16)
            | ((b[4] as u64) << 8)
            | (b[5] as u64)
    }
}
