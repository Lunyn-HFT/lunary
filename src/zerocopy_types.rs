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
        let ptr = self.timestamp6.as_ptr();
        unsafe {
            let b0 = *ptr as u64;
            let b1 = *ptr.add(1) as u64;
            let b2 = *ptr.add(2) as u64;
            let b3 = *ptr.add(3) as u64;
            let b4 = *ptr.add(4) as u64;
            let b5 = *ptr.add(5) as u64;
            (b0 << 40) | (b1 << 32) | (b2 << 24) | (b3 << 16) | (b4 << 8) | b5
        }
    }
}
