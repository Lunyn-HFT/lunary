use zerocopy::byteorder::network_endian::U16;

#[derive(zerocopy::FromBytes, zerocopy::Unaligned, zerocopy::Immutable, zerocopy::KnownLayout)]
#[repr(C)]
pub struct MessageHeaderRaw {
    pub stock_locate: U16,
    pub tracking_number: U16,
    pub timestamp6: [u8; 6],
}

impl MessageHeaderRaw {
    #[inline(always)]
    pub fn stock_locate(&self) -> u16 {
        self.stock_locate.get()
    }

    #[inline(always)]
    pub fn tracking_number(&self) -> u16 {
        self.tracking_number.get()
    }

    #[inline(always)]
    pub fn timestamp(&self) -> u64 {
        let ts = &self.timestamp6;
        ((ts[0] as u64) << 40)
            | ((ts[1] as u64) << 32)
            | ((ts[2] as u64) << 24)
            | ((ts[3] as u64) << 16)
            | ((ts[4] as u64) << 8)
            | (ts[5] as u64)
    }
}
