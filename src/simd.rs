#[cfg(all(feature = "simd", target_arch = "x86_64"))]
use std::arch::x86_64::*;

use std::sync::atomic::{AtomicU8, Ordering};

const SIMD_UNKNOWN: u8 = 0;
const SIMD_AVAILABLE: u8 = 1;
const SIMD_UNAVAILABLE: u8 = 2;
const AVX512_UNKNOWN: u8 = 0;
const AVX512_AVAILABLE: u8 = 1;
const AVX512_UNAVAILABLE: u8 = 2;

static SIMD_STATUS: AtomicU8 = AtomicU8::new(SIMD_UNKNOWN);
static AVX512_STATUS: AtomicU8 = AtomicU8::new(AVX512_UNKNOWN);

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline]
fn detect_simd_features() -> bool {
    is_x86_feature_detected!("sse2") && is_x86_feature_detected!("ssse3")
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline]
fn detect_simd_features() -> bool {
    false
}

#[inline]
pub fn is_simd_available() -> bool {
    let status = SIMD_STATUS.load(Ordering::Relaxed);
    match status {
        SIMD_AVAILABLE => true,
        SIMD_UNAVAILABLE => false,
        _ => {
            let available = detect_simd_features();
            SIMD_STATUS.store(
                if available {
                    SIMD_AVAILABLE
                } else {
                    SIMD_UNAVAILABLE
                },
                Ordering::Relaxed,
            );
            available
        }
    }
}

#[inline]
pub fn simd_info() -> SimdInfo {
    SimdInfo {
        available: is_simd_available(),
        #[cfg(all(feature = "simd", target_arch = "x86_64"))]
        sse2: is_x86_feature_detected!("sse2"),
        #[cfg(all(feature = "simd", target_arch = "x86_64"))]
        ssse3: is_x86_feature_detected!("ssse3"),
        #[cfg(all(feature = "simd", target_arch = "x86_64"))]
        avx2: is_x86_feature_detected!("avx2"),
        #[cfg(all(feature = "simd", target_arch = "x86_64"))]
        avx512: is_avx512_available(),
        #[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
        sse2: false,
        #[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
        ssse3: false,
        #[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
        avx2: false,
        #[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
        avx512: false,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SimdInfo {
    pub available: bool,
    pub sse2: bool,
    pub ssse3: bool,
    pub avx2: bool,
    pub avx512: bool,
}

impl SimdInfo {
    pub fn best_available(&self) -> SimdLevel {
        if self.avx512 {
            SimdLevel::Avx512
        } else if self.avx2 {
            SimdLevel::Avx2
        } else if self.ssse3 {
            SimdLevel::Ssse3
        } else if self.sse2 {
            SimdLevel::Sse2
        } else {
            SimdLevel::Scalar
        }
    }

    pub fn register_width(&self) -> usize {
        match self.best_available() {
            SimdLevel::Avx512 => 64,
            SimdLevel::Avx2 => 32,
            SimdLevel::Sse2 | SimdLevel::Ssse3 => 16,
            SimdLevel::Scalar => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimdLevel {
    Scalar,
    Sse2,
    Ssse3,
    Avx2,
    Avx512,
}

impl std::fmt::Display for SimdLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimdLevel::Scalar => write!(f, "Scalar"),
            SimdLevel::Sse2 => write!(f, "SSE2"),
            SimdLevel::Ssse3 => write!(f, "SSSE3"),
            SimdLevel::Avx2 => write!(f, "AVX2"),
            SimdLevel::Avx512 => write!(f, "AVX-512"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SimdDiagnostics {
    pub bytes_processed: u64,
    pub simd_bytes: u64,
    pub scalar_bytes: u64,
    pub messages_scanned: u64,
    pub prefetch_count: u64,
    pub level_used: Option<SimdLevel>,
}

impl SimdDiagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn simd_utilization(&self) -> f64 {
        if self.bytes_processed > 0 {
            self.simd_bytes as f64 / self.bytes_processed as f64
        } else {
            0.0
        }
    }

    pub fn record_simd(&mut self, bytes: u64, level: SimdLevel) {
        self.bytes_processed += bytes;
        self.simd_bytes += bytes;
        self.level_used = Some(level);
    }

    pub fn record_scalar(&mut self, bytes: u64) {
        self.bytes_processed += bytes;
        self.scalar_bytes += bytes;
    }

    pub fn record_message(&mut self) {
        self.messages_scanned += 1;
    }

    pub fn record_prefetch(&mut self) {
        self.prefetch_count += 1;
    }

    pub fn merge(&mut self, other: &SimdDiagnostics) {
        self.bytes_processed += other.bytes_processed;
        self.simd_bytes += other.simd_bytes;
        self.scalar_bytes += other.scalar_bytes;
        self.messages_scanned += other.messages_scanned;
        self.prefetch_count += other.prefetch_count;
        if other.level_used > self.level_used {
            self.level_used = other.level_used;
        }
    }
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[inline]
fn detect_avx512_features() -> bool {
    is_x86_feature_detected!("avx512f") && is_x86_feature_detected!("avx512bw")
}

#[cfg(not(all(feature = "avx512", target_arch = "x86_64")))]
#[inline]
fn detect_avx512_features() -> bool {
    false
}

#[inline]
pub fn is_avx512_available() -> bool {
    let status = AVX512_STATUS.load(Ordering::Relaxed);
    match status {
        AVX512_AVAILABLE => true,
        AVX512_UNAVAILABLE => false,
        _ => {
            let available = detect_avx512_features();
            AVX512_STATUS.store(
                if available {
                    AVX512_AVAILABLE
                } else {
                    AVX512_UNAVAILABLE
                },
                Ordering::Relaxed,
            );
            available
        }
    }
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
fn copy_8_sse2(dst: &mut [u8; 8], src: &[u8]) {
    unsafe {
        let v = _mm_loadl_epi64(src.as_ptr() as *const __m128i);
        _mm_storel_epi64(dst.as_mut_ptr() as *mut __m128i, v);
    }
}

#[inline(always)]
pub fn copy_8(dst: &mut [u8; 8], src: &[u8]) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && src.len() >= 8 {
            unsafe { copy_8_sse2(dst, src) };
            return;
        }
    }
    dst.copy_from_slice(&src[..8]);
}

#[inline(always)]
pub fn copy_4(dst: &mut [u8; 4], src: &[u8]) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && src.len() >= 4 {
            unsafe {
                let v = std::ptr::read_unaligned(src.as_ptr() as *const u32);
                std::ptr::write_unaligned(dst.as_mut_ptr() as *mut u32, v);
            }
            return;
        }
    }
    dst.copy_from_slice(&src[..4]);
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
fn find_message_boundary_sse2(data: &[u8], pattern: u8) -> Option<usize> {
    unsafe {
        let needle = _mm_set1_epi8(pattern as i8);
        let mut offset = 0;

        while offset + 16 <= data.len() {
            let chunk = _mm_loadu_si128(data[offset..].as_ptr() as *const __m128i);
            let cmp = _mm_cmpeq_epi8(chunk, needle);
            let mask = _mm_movemask_epi8(cmp) as u32;

            if mask != 0 {
                return Some(offset + mask.trailing_zeros() as usize);
            }
            offset += 16;
        }

        data[offset..]
            .iter()
            .position(|&b| b == pattern)
            .map(|i| i + offset)
    }
}

#[inline(always)]
pub fn find_message_boundary(data: &[u8], pattern: u8) -> Option<usize> {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && data.len() >= 16 {
            return unsafe { find_message_boundary_sse2(data, pattern) };
        }
    }

    data.iter().position(|&b| b == pattern)
}

#[inline(always)]
pub fn validate_message_types(data: &[u8], valid_types: &[u8; 256]) -> bool {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && data.len() >= 16 {
            for chunk in data.chunks(16) {
                for &byte in chunk {
                    if valid_types[byte as usize] == 0 {
                        return false;
                    }
                }
            }
            return true;
        }
    }

    data.iter().all(|&b| valid_types[b as usize] != 0)
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
fn prefetch_data_sse2(ptr: *const u8) {
    unsafe { _mm_prefetch(ptr as *const i8, _MM_HINT_T0) };
}

#[inline(always)]
/// # Safety
/// The caller must ensure that `ptr` is a valid pointer for prefetching operations.
pub unsafe fn prefetch_data(ptr: *const u8) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() {
            unsafe { prefetch_data_sse2(ptr) };
        }
    }
    let _ = ptr;
}

#[inline(always)]
pub fn read_u64_be_simd(data: &[u8]) -> u64 {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && data.len() >= 8 {
            unsafe {
                let v = std::ptr::read_unaligned(data.as_ptr() as *const u64);
                return v.to_be();
            }
        }
    }

    u64::from_be_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ])
}

#[inline(always)]
pub fn read_u32_be_simd(data: &[u8]) -> u32 {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && data.len() >= 4 {
            unsafe {
                let v = std::ptr::read_unaligned(data.as_ptr() as *const u32);
                return v.to_be();
            }
        }
    }

    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

#[inline(always)]
pub fn read_u16_be_simd(data: &[u8]) -> u16 {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && data.len() >= 2 {
            unsafe {
                let v = std::ptr::read_unaligned(data.as_ptr() as *const u16);
                return v.to_be();
            }
        }
    }

    u16::from_be_bytes([data[0], data[1]])
}

#[inline(always)]
pub fn read_timestamp_simd(data: &[u8]) -> u64 {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && data.len() >= 6 {
            let b0 = data[0] as u64;
            let b1 = data[1] as u64;
            let b2 = data[2] as u64;
            let b3 = data[3] as u64;
            let b4 = data[4] as u64;
            let b5 = data[5] as u64;
            return (b0 << 40) | (b1 << 32) | (b2 << 24) | (b3 << 16) | (b4 << 8) | b5;
        }
    }

    u64::from_be_bytes([0, 0, data[0], data[1], data[2], data[3], data[4], data[5]])
}

#[inline(always)]
pub fn prefetch_next_message(data: &[u8], offset: usize) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && offset < data.len() {
            unsafe {
                let ptr = data.as_ptr().add(offset);
                _mm_prefetch(ptr as *const i8, _MM_HINT_T0);
                if offset + 64 < data.len() {
                    _mm_prefetch(ptr.add(64) as *const i8, _MM_HINT_T1);
                }
            }
        }
    }
    let _ = (data, offset);
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
fn prefetch_range_sse2(data: &[u8]) {
    unsafe {
        let ptr = data.as_ptr();
        let len = data.len();
        let mut offset = 0;
        while offset < len {
            _mm_prefetch(ptr.add(offset) as *const i8, _MM_HINT_T0);
            offset += 64;
        }
    }
}

#[inline(always)]
pub fn prefetch_range(data: &[u8]) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() {
            unsafe { prefetch_range_sse2(data) };
        }
    }
    let _ = data;
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
fn prefetch_for_write_sse2(dst: &mut [u8], offset: usize) {
    unsafe {
        let ptr = dst.as_mut_ptr().add(offset);
        _mm_prefetch(ptr as *const i8, _MM_HINT_ET0);
    }
}

#[inline(always)]
pub fn prefetch_for_write(dst: &mut [u8], offset: usize) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && offset < dst.len() {
            unsafe { prefetch_for_write_sse2(dst, offset) };
        }
    }
    let _ = (dst, offset);
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
fn memcpy_simd_sse2(dst: &mut [u8], src: &[u8], len: usize) {
    unsafe {
        let mut i = 0;
        while i + 16 <= len {
            let v = _mm_loadu_si128(src[i..].as_ptr() as *const __m128i);
            _mm_storeu_si128(dst[i..].as_mut_ptr() as *mut __m128i, v);
            i += 16;
        }
        if i < len {
            dst[i..len].copy_from_slice(&src[i..len]);
        }
    }
}

#[inline(always)]
pub fn memcpy_simd(dst: &mut [u8], src: &[u8]) {
    let len = dst.len().min(src.len());
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_simd_available() && len >= 16 {
            unsafe { memcpy_simd_sse2(dst, src, len) };
            return;
        }
    }
    dst[..len].copy_from_slice(&src[..len]);
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline]
fn is_avx2_available() -> bool {
    is_x86_feature_detected!("avx2")
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline]
#[allow(dead_code)]
fn is_avx2_available() -> bool {
    false
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
fn memcpy_avx2_inner(dst: &mut [u8], src: &[u8], len: usize) {
    unsafe {
        let mut i = 0;
        while i + 32 <= len {
            let v = _mm256_loadu_si256(src[i..].as_ptr() as *const __m256i);
            _mm256_storeu_si256(dst[i..].as_mut_ptr() as *mut __m256i, v);
            i += 32;
        }
        while i + 16 <= len {
            let v = _mm_loadu_si128(src[i..].as_ptr() as *const __m128i);
            _mm_storeu_si128(dst[i..].as_mut_ptr() as *mut __m128i, v);
            i += 16;
        }
        if i < len {
            dst[i..len].copy_from_slice(&src[i..len]);
        }
    }
}

#[inline(always)]
pub fn memcpy_avx2(dst: &mut [u8], src: &[u8]) {
    let len = dst.len().min(src.len());
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_avx2_available() && len >= 32 {
            unsafe { memcpy_avx2_inner(dst, src, len) };
            return;
        }
    }
    let _ = len;
    memcpy_simd(dst, src);
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
fn find_bytes_avx2_inner(data: &[u8], pattern: u8) -> Option<usize> {
    unsafe {
        let needle = _mm256_set1_epi8(pattern as i8);
        let mut offset = 0;

        while offset + 32 <= data.len() {
            let chunk = _mm256_loadu_si256(data[offset..].as_ptr() as *const __m256i);
            let cmp = _mm256_cmpeq_epi8(chunk, needle);
            let mask = _mm256_movemask_epi8(cmp) as u32;

            if mask != 0 {
                return Some(offset + mask.trailing_zeros() as usize);
            }
            offset += 32;
        }

        find_message_boundary(&data[offset..], pattern).map(|i| i + offset)
    }
}

#[inline(always)]
pub fn find_bytes_avx2(data: &[u8], pattern: u8) -> Option<usize> {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        if is_avx2_available() && data.len() >= 32 {
            return unsafe { find_bytes_avx2_inner(data, pattern) };
        }
    }

    find_message_boundary(data, pattern)
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
fn memcpy_avx512_inner(dst: &mut [u8], src: &[u8], len: usize) {
    unsafe {
        let mut i = 0;
        while i + 64 <= len {
            let v = _mm512_loadu_si512(src[i..].as_ptr() as *const __m512i);
            _mm512_storeu_si512(dst[i..].as_mut_ptr() as *mut __m512i, v);
            i += 64;
        }
        while i + 32 <= len {
            let v = _mm256_loadu_si256(src[i..].as_ptr() as *const __m256i);
            _mm256_storeu_si256(dst[i..].as_mut_ptr() as *mut __m256i, v);
            i += 32;
        }
        if i < len {
            dst[i..len].copy_from_slice(&src[i..len]);
        }
    }
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[inline(always)]
pub fn memcpy_avx512(dst: &mut [u8], src: &[u8]) {
    let len = dst.len().min(src.len());
    if is_avx512_available() && len >= 64 {
        unsafe { memcpy_avx512_inner(dst, src, len) };
        return;
    }
    memcpy_avx2(dst, src);
}

#[cfg(not(all(feature = "avx512", target_arch = "x86_64")))]
#[inline(always)]
pub fn memcpy_avx512(dst: &mut [u8], src: &[u8]) {
    memcpy_avx2(dst, src);
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[target_feature(enable = "avx512bw")]
fn find_bytes_avx512_inner(data: &[u8], pattern: u8) -> Option<usize> {
    unsafe {
        let needle = _mm512_set1_epi8(pattern as i8);
        let mut offset = 0;

        while offset + 64 <= data.len() {
            let chunk = _mm512_loadu_si512(data[offset..].as_ptr() as *const __m512i);
            let mask = _mm512_cmpeq_epi8_mask(chunk, needle);

            if mask != 0 {
                return Some(offset + mask.trailing_zeros() as usize);
            }
            offset += 64;
        }

        find_bytes_avx2(&data[offset..], pattern).map(|i| i + offset)
    }
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[inline(always)]
pub fn find_bytes_avx512(data: &[u8], pattern: u8) -> Option<usize> {
    if is_avx512_available() && data.len() >= 64 {
        return unsafe { find_bytes_avx512_inner(data, pattern) };
    }

    find_bytes_avx2(data, pattern)
}

#[cfg(not(all(feature = "avx512", target_arch = "x86_64")))]
#[inline(always)]
pub fn find_bytes_avx512(data: &[u8], pattern: u8) -> Option<usize> {
    find_bytes_avx2(data, pattern)
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[inline(always)]
pub fn read_u64_be_avx512(data: &[u8]) -> u64 {
    if is_avx512_available() && data.len() >= 8 {
        unsafe {
            let v = std::ptr::read_unaligned(data.as_ptr() as *const u64);
            return v.swap_bytes();
        }
    }
    read_u64_be_simd(data)
}

#[cfg(not(all(feature = "avx512", target_arch = "x86_64")))]
#[inline(always)]
pub fn read_u64_be_avx512(data: &[u8]) -> u64 {
    read_u64_be_simd(data)
}

#[cfg(all(feature = "avx512", target_arch = "x86_64"))]
#[inline(always)]
pub fn prefetch_avx512(data: &[u8], offset: usize) {
    if is_avx512_available() && offset < data.len() {
        unsafe {
            let ptr = data.as_ptr().add(offset);
            _mm_prefetch(ptr as *const i8, _MM_HINT_T0);
            if offset + 64 < data.len() {
                _mm_prefetch(ptr.add(64) as *const i8, _MM_HINT_T0);
            }
            if offset + 128 < data.len() {
                _mm_prefetch(ptr.add(128) as *const i8, _MM_HINT_T1);
            }
        }
    }
}

#[cfg(not(all(feature = "avx512", target_arch = "x86_64")))]
#[inline(always)]
pub fn prefetch_avx512(data: &[u8], offset: usize) {
    prefetch_next_message(data, offset);
}

#[inline(always)]
/// # Safety
///
/// Caller must ensure `pos + 8 <= data.len()`.
pub unsafe fn read_u64_unchecked(data: &[u8], pos: usize) -> u64 {
    let ptr = unsafe { data.as_ptr().add(pos) };
    u64::from_be_bytes(unsafe { std::ptr::read_unaligned(ptr as *const [u8; 8]) })
}

#[inline(always)]
/// # Safety
///
/// Caller must ensure `pos + 4 <= data.len()`.
pub unsafe fn read_u32_unchecked(data: &[u8], pos: usize) -> u32 {
    let ptr = unsafe { data.as_ptr().add(pos) };
    u32::from_be_bytes(unsafe { std::ptr::read_unaligned(ptr as *const [u8; 4]) })
}

#[inline(always)]
/// # Safety
///
/// Caller must ensure `pos + 2 <= data.len()`.
pub unsafe fn read_u16_unchecked(data: &[u8], pos: usize) -> u16 {
    let ptr = unsafe { data.as_ptr().add(pos) };
    u16::from_be_bytes(unsafe { std::ptr::read_unaligned(ptr as *const [u8; 2]) })
}

#[inline(always)]
/// # Safety
///
/// Caller must ensure `pos + 6 <= data.len()`.
pub unsafe fn read_timestamp_unchecked(data: &[u8], pos: usize) -> u64 {
    let ptr = unsafe { data.as_ptr().add(pos) };
    let b0 = unsafe { *ptr } as u64;
    let b1 = unsafe { *ptr.add(1) } as u64;
    let b2 = unsafe { *ptr.add(2) } as u64;
    let b3 = unsafe { *ptr.add(3) } as u64;
    let b4 = unsafe { *ptr.add(4) } as u64;
    let b5 = unsafe { *ptr.add(5) } as u64;
    (b0 << 40) | (b1 << 32) | (b2 << 24) | (b3 << 16) | (b4 << 8) | b5
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
pub fn scan_message_lengths_simd(data: &[u8], max_messages: usize) -> Vec<(usize, usize)> {
    let mut results = Vec::with_capacity(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    while offset + 2 <= len && results.len() < max_messages {
        if offset + 64 <= len {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
            }
        }

        let msg_len = unsafe { read_u16_unchecked(data, offset) } as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        results.push((offset, msg_len));
        offset += total;
    }

    results
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline(always)]
pub fn scan_message_lengths_simd(data: &[u8], max_messages: usize) -> Vec<(usize, usize)> {
    let mut results = Vec::with_capacity(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    while offset + 2 <= len && results.len() < max_messages {
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        results.push((offset, msg_len));
        offset += total;
    }

    results
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
unsafe fn memcpy_nontemporal_sse2(dst: *mut u8, src: *const u8, len: usize) {
    unsafe {
        let mut i = 0;
        while i + 16 <= len {
            let v = _mm_loadu_si128(src.add(i) as *const __m128i);
            _mm_stream_si128(dst.add(i) as *mut __m128i, v);
            i += 16;
        }
        _mm_sfence();
        if i < len {
            std::ptr::copy_nonoverlapping(src.add(i), dst.add(i), len - i);
        }
    }
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
/// # Safety
///
/// Caller must ensure:
/// - `src` is valid for reads of `len` bytes
/// - `dst` is valid for writes of `len` bytes and is 16-byte aligned
/// - the regions do not overlap
pub unsafe fn memcpy_nontemporal(dst: *mut u8, src: *const u8, len: usize) {
    unsafe {
        if is_simd_available() && len >= 64 {
            memcpy_nontemporal_sse2(dst, src, len);
            return;
        }
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline(always)]
/// # Safety
///
/// Caller must ensure:
/// - `src` is valid for reads of `len` bytes
/// - `dst` is valid for writes of `len` bytes
/// - the regions do not overlap
pub unsafe fn memcpy_nontemporal(dst: *mut u8, src: *const u8, len: usize) {
    std::ptr::copy_nonoverlapping(src, dst, len);
}

#[inline]
#[cold]
fn cold() {}

#[inline]
pub fn likely(b: bool) -> bool {
    if !b {
        cold();
    }
    b
}

#[inline]
pub fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
pub fn count_messages_fast(data: &[u8]) -> usize {
    let mut count = 0;
    let mut offset = 0;
    let len = data.len();

    while offset + 2 <= len {
        let msg_len = unsafe { read_u16_unchecked(data, offset) } as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        count += 1;
        offset += total;

        if offset + 128 <= len {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
                _mm_prefetch(data.as_ptr().add(offset + 128) as *const i8, _MM_HINT_T1);
            }
        }
    }

    count
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline(always)]
pub fn count_messages_fast(data: &[u8]) -> usize {
    let mut count = 0;
    let mut offset = 0;
    let len = data.len();

    while offset + 2 <= len {
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        count += 1;
        offset += total;
    }

    count
}

#[derive(Debug, Clone)]
pub struct BoundaryResult {
    pub boundaries: Vec<(usize, usize)>,
    pub diagnostics: SimdDiagnostics,
}

impl BoundaryResult {
    pub fn new(capacity: usize) -> Self {
        Self {
            boundaries: Vec::with_capacity(capacity),
            diagnostics: SimdDiagnostics::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.boundaries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.boundaries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(usize, usize)> {
        self.boundaries.iter()
    }
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn scan_boundaries_with_diagnostics(data: &[u8], max_messages: usize) -> BoundaryResult {
    let mut result = BoundaryResult::new(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    let level = simd_info().best_available();
    result.diagnostics.level_used = Some(level);

    while offset + 2 <= len && result.boundaries.len() < max_messages {
        if offset + 64 <= len {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
            }
            result.diagnostics.record_prefetch();
        }

        let msg_len = unsafe { read_u16_unchecked(data, offset) } as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        result.boundaries.push((offset, msg_len));
        result.diagnostics.record_message();
        result.diagnostics.record_simd(total as u64, level);
        offset += total;
    }

    result
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn scan_boundaries_with_diagnostics(data: &[u8], max_messages: usize) -> BoundaryResult {
    let mut result = BoundaryResult::new(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    result.diagnostics.level_used = Some(SimdLevel::Scalar);

    while offset + 2 <= len && result.boundaries.len() < max_messages {
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        result.boundaries.push((offset, msg_len));
        result.diagnostics.record_message();
        result.diagnostics.record_scalar(total as u64);
        offset += total;
    }

    result
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn scan_boundaries_avx2(data: &[u8], max_messages: usize) -> BoundaryResult {
    let mut result = BoundaryResult::new(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    if !is_simd_available() {
        return scan_boundaries_with_diagnostics(data, max_messages);
    }

    result.diagnostics.level_used = Some(SimdLevel::Avx2);

    while offset + 2 <= len && result.boundaries.len() < max_messages {
        if offset + 128 <= len && is_avx2_available() {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
                _mm_prefetch(data.as_ptr().add(offset + 128) as *const i8, _MM_HINT_T1);
            }
            result.diagnostics.prefetch_count += 2;
        }

        let msg_len = unsafe { read_u16_unchecked(data, offset) } as usize;
        let total = msg_len + 2;

        if offset + total > len {
            break;
        }

        result.boundaries.push((offset, msg_len));
        result.diagnostics.record_message();
        result
            .diagnostics
            .record_simd(total as u64, SimdLevel::Avx2);
        offset += total;
    }

    result
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn scan_boundaries_avx2(data: &[u8], max_messages: usize) -> BoundaryResult {
    scan_boundaries_with_diagnostics(data, max_messages)
}

#[inline]
pub fn scan_boundaries_auto(data: &[u8], max_messages: usize) -> BoundaryResult {
    let info = simd_info();
    match info.best_available() {
        SimdLevel::Avx512 | SimdLevel::Avx2 => scan_boundaries_avx2(data, max_messages),
        _ => scan_boundaries_with_diagnostics(data, max_messages),
    }
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn validate_boundaries_simd(data: &[u8], boundaries: &[(usize, usize)]) -> bool {
    if !is_simd_available() {
        return validate_boundaries_scalar(data, boundaries);
    }

    for &(offset, len) in boundaries {
        if offset + 2 > data.len() {
            return false;
        }

        let actual_len = unsafe { read_u16_unchecked(data, offset) } as usize;
        if actual_len != len {
            return false;
        }

        if offset + len + 2 > data.len() {
            return false;
        }
    }

    true
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn validate_boundaries_simd(data: &[u8], boundaries: &[(usize, usize)]) -> bool {
    validate_boundaries_scalar(data, boundaries)
}

pub fn validate_boundaries_scalar(data: &[u8], boundaries: &[(usize, usize)]) -> bool {
    for &(offset, len) in boundaries {
        if offset + 2 > data.len() {
            return false;
        }

        let actual_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        if actual_len != len {
            return false;
        }

        if offset + len + 2 > data.len() {
            return false;
        }
    }

    true
}

#[derive(Debug, Clone, Default)]
pub struct ParseDiagnostics {
    pub simd_reads: u64,
    pub scalar_reads: u64,
    pub prefetch_hits: u64,
    pub cache_lines_touched: u64,
    pub level_used: Option<SimdLevel>,
}

impl ParseDiagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn simd_ratio(&self) -> f64 {
        let total = self.simd_reads + self.scalar_reads;
        if total > 0 {
            self.simd_reads as f64 / total as f64
        } else {
            0.0
        }
    }

    pub fn merge(&mut self, other: &ParseDiagnostics) {
        self.simd_reads += other.simd_reads;
        self.scalar_reads += other.scalar_reads;
        self.prefetch_hits += other.prefetch_hits;
        self.cache_lines_touched += other.cache_lines_touched;
        if other.level_used > self.level_used {
            self.level_used = other.level_used;
        }
    }
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
pub fn batch_read_u16_simd(data: &[u8], offsets: &[usize], out: &mut [u16]) -> usize {
    let count = offsets.len().min(out.len());
    for i in 0..count {
        let offset = offsets[i];
        if offset + 2 <= data.len() {
            out[i] = unsafe { read_u16_unchecked(data, offset) };
        }
    }
    count
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline(always)]
pub fn batch_read_u16_simd(data: &[u8], offsets: &[usize], out: &mut [u16]) -> usize {
    let count = offsets.len().min(out.len());
    for i in 0..count {
        let offset = offsets[i];
        if offset + 2 <= data.len() {
            out[i] = u16::from_be_bytes([data[offset], data[offset + 1]]);
        }
    }
    count
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
pub fn batch_read_u32_simd(data: &[u8], offsets: &[usize], out: &mut [u32]) -> usize {
    let count = offsets.len().min(out.len());
    for i in 0..count {
        let offset = offsets[i];
        if offset + 4 <= data.len() {
            out[i] = unsafe { read_u32_unchecked(data, offset) };
        }
    }
    count
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline(always)]
pub fn batch_read_u32_simd(data: &[u8], offsets: &[usize], out: &mut [u32]) -> usize {
    let count = offsets.len().min(out.len());
    for i in 0..count {
        let offset = offsets[i];
        if offset + 4 <= data.len() {
            out[i] = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
        }
    }
    count
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[inline(always)]
pub fn batch_read_u64_simd(data: &[u8], offsets: &[usize], out: &mut [u64]) -> usize {
    let count = offsets.len().min(out.len());
    for i in 0..count {
        let offset = offsets[i];
        if offset + 8 <= data.len() {
            out[i] = unsafe { read_u64_unchecked(data, offset) };
        }
    }
    count
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
#[inline(always)]
pub fn batch_read_u64_simd(data: &[u8], offsets: &[usize], out: &mut [u64]) -> usize {
    let count = offsets.len().min(out.len());
    for i in 0..count {
        let offset = offsets[i];
        if offset + 8 <= data.len() {
            out[i] = u64::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
        }
    }
    count
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn validate_message_sequence_simd(data: &[u8], max_messages: usize) -> (bool, usize) {
    let mut offset = 0;
    let mut count = 0;
    let len = data.len();

    while offset + 3 <= len && count < max_messages {
        if offset + 64 <= len {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
            }
        }

        let msg_len = unsafe { read_u16_unchecked(data, offset) } as usize;

        if msg_len == 0 || msg_len > 64 * 1024 {
            return (false, count);
        }

        let total = msg_len + 2;
        if offset + total > len {
            return (false, count);
        }

        let msg_type = data[offset + 2];
        if !is_valid_message_type(msg_type) {
            return (false, count);
        }

        offset += total;
        count += 1;
    }

    (true, count)
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn validate_message_sequence_simd(data: &[u8], max_messages: usize) -> (bool, usize) {
    let mut offset = 0;
    let mut count = 0;
    let len = data.len();

    while offset + 3 <= len && count < max_messages {
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;

        if msg_len == 0 || msg_len > 64 * 1024 {
            return (false, count);
        }

        let total = msg_len + 2;
        if offset + total > len {
            return (false, count);
        }

        let msg_type = data[offset + 2];
        if !is_valid_message_type(msg_type) {
            return (false, count);
        }

        offset += total;
        count += 1;
    }

    (true, count)
}

#[inline]
pub fn is_valid_message_type(msg_type: u8) -> bool {
    matches!(
        msg_type,
        b'S' | b'R'
            | b'H'
            | b'Y'
            | b'L'
            | b'V'
            | b'W'
            | b'K'
            | b'A'
            | b'F'
            | b'E'
            | b'C'
            | b'X'
            | b'D'
            | b'U'
            | b'P'
            | b'Q'
            | b'B'
            | b'I'
            | b'N'
            | b'J'
    )
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn extract_timestamps_simd(data: &[u8], max_messages: usize) -> Vec<u64> {
    let mut timestamps = Vec::with_capacity(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    while offset + 13 <= len && timestamps.len() < max_messages {
        if offset + 64 <= len {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
            }
        }

        let msg_len = unsafe { read_u16_unchecked(data, offset) } as usize;
        let total = msg_len + 2;

        if offset + total > len || msg_len < 11 {
            break;
        }

        let ts = unsafe { read_timestamp_unchecked(data, offset + 7) };
        timestamps.push(ts);
        offset += total;
    }

    timestamps
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn extract_timestamps_simd(data: &[u8], max_messages: usize) -> Vec<u64> {
    let mut timestamps = Vec::with_capacity(max_messages.min(1024));
    let mut offset = 0;
    let len = data.len();

    while offset + 13 <= len && timestamps.len() < max_messages {
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let total = msg_len + 2;

        if offset + total > len || msg_len < 11 {
            break;
        }

        let ts = u64::from_be_bytes([
            0,
            0,
            data[offset + 7],
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
            data[offset + 12],
        ]);
        timestamps.push(ts);
        offset += total;
    }

    timestamps
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub cache_lines_read: u64,
    pub cache_lines_written: u64,
    pub prefetch_issued: u64,
    pub estimated_misses: u64,
    pub bytes_per_line: usize,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            bytes_per_line: 64,
            ..Default::default()
        }
    }

    pub fn with_line_size(line_size: usize) -> Self {
        Self {
            bytes_per_line: line_size,
            ..Default::default()
        }
    }

    pub fn record_read(&mut self, bytes: u64) {
        self.cache_lines_read += bytes.div_ceil(self.bytes_per_line as u64);
    }

    pub fn record_write(&mut self, bytes: u64) {
        self.cache_lines_written += bytes.div_ceil(self.bytes_per_line as u64);
    }

    pub fn record_prefetch(&mut self) {
        self.prefetch_issued += 1;
    }

    pub fn record_miss(&mut self) {
        self.estimated_misses += 1;
    }

    pub fn total_cache_lines(&self) -> u64 {
        self.cache_lines_read + self.cache_lines_written
    }

    pub fn prefetch_hit_ratio(&self) -> f64 {
        if self.cache_lines_read > 0 {
            let covered = self.prefetch_issued.min(self.cache_lines_read);
            covered as f64 / self.cache_lines_read as f64
        } else {
            0.0
        }
    }

    pub fn estimated_bandwidth(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            (self.total_cache_lines() * self.bytes_per_line as u64) as f64 / elapsed_secs / 1e9
        } else {
            0.0
        }
    }

    pub fn merge(&mut self, other: &CacheStats) {
        self.cache_lines_read += other.cache_lines_read;
        self.cache_lines_written += other.cache_lines_written;
        self.prefetch_issued += other.prefetch_issued;
        self.estimated_misses += other.estimated_misses;
    }
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn compute_checksum_simd(data: &[u8]) -> u32 {
    if !is_simd_available() || data.len() < 16 {
        return compute_checksum_scalar(data);
    }

    unsafe {
        let mut sum = _mm_setzero_si128();
        let mut offset = 0;
        let len = data.len();

        while offset + 16 <= len {
            let chunk = _mm_loadu_si128(data[offset..].as_ptr() as *const __m128i);
            let sad = _mm_sad_epu8(chunk, _mm_setzero_si128());
            sum = _mm_add_epi64(sum, sad);
            offset += 16;
        }

        let lo = _mm_extract_epi64(sum, 0) as u64;
        let hi = _mm_extract_epi64(sum, 1) as u64;
        let mut result = (lo + hi) as u32;

        for &byte in &data[offset..len] {
            result = result.wrapping_add(byte as u32);
        }

        result
    }
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn compute_checksum_simd(data: &[u8]) -> u32 {
    compute_checksum_scalar(data)
}

pub fn compute_checksum_scalar(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for &byte in data {
        sum = sum.wrapping_add(byte as u32);
    }
    sum
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn validate_checksum_simd(data: &[u8], expected: u32) -> bool {
    compute_checksum_simd(data) == expected
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn validate_checksum_simd(data: &[u8], expected: u32) -> bool {
    compute_checksum_scalar(data) == expected
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn batch_validate_messages_simd(
    data: &[u8],
    boundaries: &[(usize, usize)],
    expected_checksums: Option<&[u32]>,
) -> Vec<bool> {
    let mut results = Vec::with_capacity(boundaries.len());

    for (i, &(offset, len)) in boundaries.iter().enumerate() {
        if offset + len + 2 > data.len() {
            results.push(false);
            continue;
        }

        let msg_data = &data[offset..offset + len + 2];

        if expected_checksums.is_some_and(|checksums| i < checksums.len()) {
            let checksums = expected_checksums.as_ref().unwrap();
            results.push(compute_checksum_simd(msg_data) == checksums[i]);
            continue;
        }

        let actual_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        results.push(actual_len == len);
    }

    results
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn batch_validate_messages_simd(
    data: &[u8],
    boundaries: &[(usize, usize)],
    expected_checksums: Option<&[u32]>,
) -> Vec<bool> {
    let mut results = Vec::with_capacity(boundaries.len());

    for (i, &(offset, len)) in boundaries.iter().enumerate() {
        if offset + len + 2 > data.len() {
            results.push(false);
            continue;
        }

        let msg_data = &data[offset..offset + len + 2];

        if let Some(checksums) = expected_checksums {
            if i < checksums.len() {
                results.push(compute_checksum_scalar(msg_data) == checksums[i]);
                continue;
            }
        }

        let actual_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        results.push(actual_len == len);
    }

    results
}

#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub valid_count: u64,
    pub invalid_count: u64,
    pub error_offsets: Vec<usize>,
    pub error_types: Vec<ValidationError>,
    pub bytes_validated: u64,
    pub simd_level: Option<SimdLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    InvalidLength,
    InvalidMessageType,
    TruncatedMessage,
    InvalidTimestamp,
    ChecksumMismatch,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_valid(&self) -> bool {
        self.invalid_count == 0
    }

    pub fn error_rate(&self) -> f64 {
        let total = self.valid_count + self.invalid_count;
        if total > 0 {
            self.invalid_count as f64 / total as f64
        } else {
            0.0
        }
    }

    pub fn merge(&mut self, other: &ValidationResult) {
        self.valid_count += other.valid_count;
        self.invalid_count += other.invalid_count;
        self.error_offsets.extend(&other.error_offsets);
        self.error_types.extend(&other.error_types);
        self.bytes_validated += other.bytes_validated;
        if other.simd_level > self.simd_level {
            self.simd_level = other.simd_level;
        }
    }
}

const VALID_MSG_TYPES: [u8; 22] = [
    b'S', b'R', b'H', b'Y', b'L', b'V', b'W', b'K', b'A', b'F', b'E', b'C', b'X', b'D', b'U', b'P',
    b'Q', b'B', b'I', b'N', b'J', b'h',
];

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
pub fn validate_message_stream_simd(data: &[u8], max_errors: usize) -> ValidationResult {
    let mut result = ValidationResult::new();
    let mut offset = 0;
    let len = data.len();

    result.simd_level = Some(simd_info().best_available());

    while offset + 3 <= len {
        if offset + 64 <= len {
            unsafe {
                _mm_prefetch(data.as_ptr().add(offset + 64) as *const i8, _MM_HINT_T0);
            }
        }

        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let total = msg_len + 2;

        if msg_len == 0 || msg_len > 64 * 1024 {
            result.invalid_count += 1;
            result.error_offsets.push(offset);
            result.error_types.push(ValidationError::InvalidLength);
            if result.error_offsets.len() >= max_errors {
                break;
            }
            offset += 1;
            continue;
        }

        if offset + total > len {
            result.invalid_count += 1;
            result.error_offsets.push(offset);
            result.error_types.push(ValidationError::TruncatedMessage);
            break;
        }

        let msg_type = data[offset + 2];
        if !VALID_MSG_TYPES.contains(&msg_type) {
            result.invalid_count += 1;
            result.error_offsets.push(offset);
            result.error_types.push(ValidationError::InvalidMessageType);
            if result.error_offsets.len() >= max_errors {
                break;
            }
        } else {
            result.valid_count += 1;
        }

        result.bytes_validated += total as u64;
        offset += total;
    }

    result
}

#[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
pub fn validate_message_stream_simd(data: &[u8], max_errors: usize) -> ValidationResult {
    let mut result = ValidationResult::new();
    let mut offset = 0;
    let len = data.len();

    result.simd_level = Some(SimdLevel::Scalar);

    while offset + 3 <= len {
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let total = msg_len + 2;

        if msg_len == 0 || msg_len > 64 * 1024 {
            result.invalid_count += 1;
            result.error_offsets.push(offset);
            result.error_types.push(ValidationError::InvalidLength);
            if result.error_offsets.len() >= max_errors {
                break;
            }
            offset += 1;
            continue;
        }

        if offset + total > len {
            result.invalid_count += 1;
            result.error_offsets.push(offset);
            result.error_types.push(ValidationError::TruncatedMessage);
            break;
        }

        let msg_type = data[offset + 2];
        if !VALID_MSG_TYPES.contains(&msg_type) {
            result.invalid_count += 1;
            result.error_offsets.push(offset);
            result.error_types.push(ValidationError::InvalidMessageType);
            if result.error_offsets.len() >= max_errors {
                break;
            }
        } else {
            result.valid_count += 1;
        }

        result.bytes_validated += total as u64;
        offset += total;
    }

    result
}

#[derive(Debug, Clone, Default)]
pub struct ParseDiagnosticsExt {
    pub simd_diagnostics: SimdDiagnostics,
    pub cache_stats: CacheStats,
    pub validation_result: ValidationResult,
    pub parse_time_ns: u64,
    pub throughput_gbps: f64,
}

impl ParseDiagnosticsExt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compute_throughput(&mut self) {
        if self.parse_time_ns > 0 {
            let bytes = self.simd_diagnostics.bytes_processed as f64;
            let secs = self.parse_time_ns as f64 / 1e9;
            self.throughput_gbps = bytes / secs / 1e9;
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "SIMD: {} ({:.1}% util), Cache: {} lines ({:.1}% prefetch), Validation: {}/{} ok ({:.2}% errors), {:.2} GB/s",
            self.simd_diagnostics
                .level_used
                .map(|l| l.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            self.simd_diagnostics.simd_utilization() * 100.0,
            self.cache_stats.total_cache_lines(),
            self.cache_stats.prefetch_hit_ratio() * 100.0,
            self.validation_result.valid_count,
            self.validation_result.valid_count + self.validation_result.invalid_count,
            self.validation_result.error_rate() * 100.0,
            self.throughput_gbps
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    mod fixtures {
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
    }

    #[test]
    fn test_simd_detection() {
        let info = simd_info();
        let _ = info.best_available();
    }

    #[test]
    fn test_read_u16_unchecked() {
        let data = [0x12, 0x34, 0x56, 0x78];
        unsafe {
            assert_eq!(read_u16_unchecked(&data, 0), 0x1234);
            assert_eq!(read_u16_unchecked(&data, 2), 0x5678);
        }
    }

    #[test]
    fn test_read_u32_unchecked() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        unsafe {
            assert_eq!(read_u32_unchecked(&data, 0), 0x12345678);
            assert_eq!(read_u32_unchecked(&data, 2), 0x56789ABC);
        }
    }

    #[test]
    fn test_read_u64_unchecked() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22];
        unsafe {
            assert_eq!(read_u64_unchecked(&data, 0), 0x123456789ABCDEF0);
            assert_eq!(read_u64_unchecked(&data, 2), 0x56789ABCDEF01122);
        }
    }

    #[test]
    fn test_read_timestamp_unchecked() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
        unsafe {
            assert_eq!(read_timestamp_unchecked(&data, 0), 0x123456789ABC);
        }
    }

    #[test]
    fn test_memcpy_nontemporal_small() {
        let src = [1, 2, 3, 4, 5];
        let mut dst = [0; 5];
        unsafe {
            memcpy_nontemporal(dst.as_mut_ptr(), src.as_ptr(), 5);
        }
        assert_eq!(dst, src);
    }

    #[test]
    fn test_memcpy_nontemporal_large() {
        use std::alloc::{Layout, alloc, dealloc};
        let layout = Layout::from_size_align(128, 16).unwrap();
        let dst_ptr = unsafe { alloc(layout) };
        let src = [42; 128];
        unsafe {
            memcpy_nontemporal(dst_ptr, src.as_ptr(), 128);
            for i in 0..128 {
                assert_eq!(*dst_ptr.add(i), 42);
            }
            dealloc(dst_ptr, layout);
        }
    }

    #[test]
    fn test_simd_level_fallback() {
        let info = SimdInfo {
            available: false,
            sse2: false,
            ssse3: false,
            avx2: false,
            avx512: false,
        };
        assert_eq!(info.register_width(), 8);
    }

    #[test]
    fn test_simd_scalar_consistency_copy_8() {
        let src = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut dst_simd = [0u8; 8];
        let mut dst_scalar = [0u8; 8];

        copy_8(&mut dst_simd, &src);
        dst_scalar.copy_from_slice(&src[..8]);

        assert_eq!(dst_simd, dst_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_copy_4() {
        let src = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut dst_simd = [0u8; 4];
        let mut dst_scalar = [0u8; 4];

        copy_4(&mut dst_simd, &src);
        dst_scalar.copy_from_slice(&src[..4]);

        assert_eq!(dst_simd, dst_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_read_u64_be() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let expected = u64::from_be_bytes(data);

        let result_simd = read_u64_be_simd(&data);
        assert_eq!(result_simd, expected);
    }

    #[test]
    fn test_simd_scalar_consistency_read_u32_be() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let expected = u32::from_be_bytes(data);

        let result_simd = read_u32_be_simd(&data);
        assert_eq!(result_simd, expected);
    }

    #[test]
    fn test_simd_scalar_consistency_read_u16_be() {
        let data = [0x12, 0x34];
        let expected = u16::from_be_bytes(data);

        let result_simd = read_u16_be_simd(&data);
        assert_eq!(result_simd, expected);
    }

    #[test]
    fn test_simd_scalar_consistency_read_timestamp() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        let expected = u64::from_be_bytes([0, 0, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        let result_simd = read_timestamp_simd(&data);
        assert_eq!(result_simd, expected);
    }

    #[test]
    fn test_simd_scalar_consistency_find_message_boundary() {
        let data = [0, 1, 2, 255, 4, 5, 6, 7, 8, 9];
        let pattern = 255u8;

        let result_simd = find_message_boundary(&data, pattern);
        let result_scalar = data.iter().position(|&b| b == pattern);

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_memcpy() {
        let src = vec![42; 64];
        let mut dst_simd = vec![0; 64];
        let mut dst_scalar = vec![0; 64];

        memcpy_simd(&mut dst_simd, &src);
        dst_scalar.copy_from_slice(&src);

        assert_eq!(dst_simd, dst_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_memcpy_avx2() {
        let src = vec![42; 64];
        let mut dst_simd = vec![0; 64];
        let mut dst_scalar = vec![0; 64];

        memcpy_avx2(&mut dst_simd, &src);
        dst_scalar.copy_from_slice(&src);

        assert_eq!(dst_simd, dst_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_memcpy_avx512() {
        let src = vec![42; 128];
        let mut dst_simd = vec![0; 128];
        let mut dst_scalar = vec![0; 128];

        memcpy_avx512(&mut dst_simd, &src);
        dst_scalar.copy_from_slice(&src);

        assert_eq!(dst_simd, dst_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_find_bytes_avx2() {
        let data = [0, 1, 2, 255, 4, 5, 6, 7, 8, 9, 255, 11];
        let pattern = 255u8;

        let result_simd = find_bytes_avx2(&data, pattern);
        let result_scalar = find_message_boundary(&data, pattern);

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_find_bytes_avx512() {
        let data = [0, 1, 2, 255, 4, 5, 6, 7, 8, 9, 255, 11];
        let pattern = 255u8;

        let result_simd = find_bytes_avx512(&data, pattern);
        let result_scalar = find_message_boundary(&data, pattern);

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_compute_checksum() {
        let data = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
        ];

        let result_simd = compute_checksum_simd(&data);
        let result_scalar = compute_checksum_scalar(&data);

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_validate_checksum() {
        let data = [1, 2, 3, 4, 5];
        let checksum = compute_checksum_scalar(&data);

        let result_simd = validate_checksum_simd(&data, checksum);
        let result_scalar = compute_checksum_scalar(&data) == checksum;

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_count_messages() {
        let data = fixtures::standard_fixture();

        let result_simd = count_messages_fast(&data);
        let mut result_scalar = 0;
        let mut offset = 0;
        while offset + 2 <= data.len() {
            let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            let total = msg_len + 2;
            if offset + total > data.len() {
                break;
            }
            result_scalar += 1;
            offset += total;
        }

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_scan_boundaries() {
        let data = fixtures::standard_fixture();

        let result_simd = scan_boundaries_with_diagnostics(&data, usize::MAX);
        let mut boundaries_scalar = Vec::new();
        let mut offset = 0;
        while offset + 2 <= data.len() {
            let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            let total = msg_len + 2;
            if offset + total > data.len() {
                break;
            }
            boundaries_scalar.push((offset, msg_len));
            offset += total;
        }

        assert_eq!(result_simd.boundaries, boundaries_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_validate_boundaries() {
        let data = fixtures::standard_fixture();
        let boundaries = vec![(0, 11), (13, 12), (27, 13)];

        let result_simd = validate_boundaries_simd(&data, &boundaries);
        let result_scalar = validate_boundaries_scalar(&data, &boundaries);

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_batch_read_u16() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        let offsets = [0, 2, 4];
        let mut out_simd = [0u16; 3];
        let mut out_scalar = [0u16; 3];

        let count_simd = batch_read_u16_simd(&data, &offsets, &mut out_simd);
        let count_scalar = offsets.len().min(out_scalar.len());
        for i in 0..count_scalar {
            let offset = offsets[i];
            if offset + 2 <= data.len() {
                out_scalar[i] = u16::from_be_bytes([data[offset], data[offset + 1]]);
            }
        }

        assert_eq!(count_simd, count_scalar);
        assert_eq!(out_simd[..count_simd], out_scalar[..count_scalar]);
    }

    #[test]
    fn test_simd_scalar_consistency_batch_read_u32() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let offsets = [0, 4];
        let mut out_simd = [0u32; 2];
        let mut out_scalar = [0u32; 2];

        let count_simd = batch_read_u32_simd(&data, &offsets, &mut out_simd);
        let count_scalar = offsets.len().min(out_scalar.len());
        for i in 0..count_scalar {
            let offset = offsets[i];
            if offset + 4 <= data.len() {
                out_scalar[i] = u32::from_be_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]);
            }
        }

        assert_eq!(count_simd, count_scalar);
        assert_eq!(out_simd[..count_simd], out_scalar[..count_scalar]);
    }

    #[test]
    fn test_simd_scalar_consistency_batch_read_u64() {
        let data = [
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44,
        ];
        let offsets = [0, 4];
        let mut out_simd = [0u64; 2];
        let mut out_scalar = [0u64; 2];

        let count_simd = batch_read_u64_simd(&data, &offsets, &mut out_simd);
        let count_scalar = offsets.len().min(out_scalar.len());
        for i in 0..count_scalar {
            let offset = offsets[i];
            if offset + 8 <= data.len() {
                out_scalar[i] = u64::from_be_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]);
            }
        }

        assert_eq!(count_simd, count_scalar);
        assert_eq!(out_simd[..count_simd], out_scalar[..count_scalar]);
    }

    #[test]
    fn test_simd_scalar_consistency_validate_message_sequence() {
        let data = fixtures::standard_fixture();

        let (valid_simd, count_simd) = validate_message_sequence_simd(&data, usize::MAX);
        let mut valid_scalar = true;
        let mut count_scalar = 0;
        let mut offset = 0;
        let len = data.len();

        while offset + 3 <= len && count_scalar < usize::MAX {
            let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;

            if msg_len == 0 || msg_len > 64 * 1024 {
                valid_scalar = false;
                break;
            }

            let total = msg_len + 2;
            if offset + total > len {
                valid_scalar = false;
                break;
            }

            let msg_type = data[offset + 2];
            if !is_valid_message_type(msg_type) {
                valid_scalar = false;
                break;
            }

            offset += total;
            count_scalar += 1;
        }

        assert_eq!(valid_simd, valid_scalar);
        assert_eq!(count_simd, count_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_extract_timestamps() {
        let data = fixtures::standard_fixture();

        let result_simd = extract_timestamps_simd(&data, usize::MAX);
        let mut result_scalar = Vec::new();
        let mut offset = 0;
        let len = data.len();

        while offset + 13 <= len && result_scalar.len() < usize::MAX {
            let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            let total = msg_len + 2;

            if offset + total > len || msg_len < 11 {
                break;
            }

            let ts = u64::from_be_bytes([
                0,
                0,
                data[offset + 7],
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
                data[offset + 12],
            ]);
            result_scalar.push(ts);
            offset += total;
        }

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_simd_scalar_consistency_validate_message_stream() {
        let data = fixtures::standard_fixture();

        let result_simd = validate_message_stream_simd(&data, usize::MAX);
        let mut result_scalar = ValidationResult::new();
        let mut offset = 0;
        let len = data.len();

        while offset + 3 <= len {
            let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            let total = msg_len + 2;

            if msg_len == 0 || msg_len > 64 * 1024 {
                result_scalar.invalid_count += 1;
                result_scalar.error_offsets.push(offset);
                result_scalar
                    .error_types
                    .push(ValidationError::InvalidLength);
                break;
            }

            if offset + total > len {
                result_scalar.invalid_count += 1;
                result_scalar.error_offsets.push(offset);
                result_scalar
                    .error_types
                    .push(ValidationError::TruncatedMessage);
                break;
            }

            let msg_type = data[offset + 2];
            if !VALID_MSG_TYPES.contains(&msg_type) {
                result_scalar.invalid_count += 1;
                result_scalar.error_offsets.push(offset);
                result_scalar
                    .error_types
                    .push(ValidationError::InvalidMessageType);
            } else {
                result_scalar.valid_count += 1;
            }

            result_scalar.bytes_validated += total as u64;
            offset += total;
        }

        assert_eq!(result_simd.valid_count, result_scalar.valid_count);
        assert_eq!(result_simd.invalid_count, result_scalar.invalid_count);
        assert_eq!(result_simd.error_offsets, result_scalar.error_offsets);
        assert_eq!(result_simd.error_types, result_scalar.error_types);
        assert_eq!(result_simd.bytes_validated, result_scalar.bytes_validated);
    }

    #[test]
    fn test_simd_scalar_consistency_batch_validate_messages() {
        let data = fixtures::standard_fixture();
        let boundaries = vec![(0, 11), (13, 12), (27, 13)];
        let checksums = vec![1234, 5678, 9012];

        let result_simd = batch_validate_messages_simd(&data, &boundaries, Some(&checksums));
        let mut result_scalar = Vec::with_capacity(boundaries.len());

        for (i, &(offset, len)) in boundaries.iter().enumerate() {
            if offset + len + 2 > data.len() {
                result_scalar.push(false);
                continue;
            }

            let msg_data = &data[offset..offset + len + 2];
            let checksum = compute_checksum_scalar(msg_data);
            result_scalar.push(checksum == checksums[i]);
        }

        assert_eq!(result_simd, result_scalar);
    }

    #[test]
    fn test_fallback_behavior_on_non_x86() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = [0u8; 8];

        copy_8(&mut dst, &data);
        assert_eq!(dst, [1, 2, 3, 4, 5, 6, 7, 8]);

        let result = read_u64_be_simd(&data);
        let expected = u64::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_edge_cases_empty_data() {
        let empty: [u8; 0] = [];
        assert_eq!(find_message_boundary(&empty, 255), None);
        assert_eq!(compute_checksum_simd(&empty), 0);
        assert_eq!(count_messages_fast(&empty), 0);
    }

    #[test]
    fn test_edge_cases_small_data() {
        let small = [1, 2];
        let _ = find_message_boundary(&small, 255);
        assert_eq!(compute_checksum_simd(&small), 3);
    }

    #[test]
    fn test_prefetch_safety() {
        let mut data = vec![42; 100];
        unsafe { prefetch_data(data.as_ptr()) };
        prefetch_next_message(&data, 50);
        prefetch_range(&data);
        prefetch_for_write(data.as_mut_slice(), 50);
    }

    #[test]
    fn test_simd_info_consistency() {
        let info = simd_info();
        let best = info.best_available();

        match best {
            SimdLevel::Scalar
            | SimdLevel::Sse2
            | SimdLevel::Ssse3
            | SimdLevel::Avx2
            | SimdLevel::Avx512 => (),
        }

        let width = info.register_width();
        assert!((8..=64).contains(&width));
    }

    #[test]
    fn test_diagnostics_merge() {
        let mut diag1 = SimdDiagnostics::new();
        diag1.record_simd(100, SimdLevel::Avx2);
        diag1.record_scalar(50);

        let mut diag2 = SimdDiagnostics::new();
        diag2.record_simd(200, SimdLevel::Avx512);
        diag2.record_prefetch();

        diag1.merge(&diag2);

        assert_eq!(diag1.bytes_processed, 350);
        assert_eq!(diag1.simd_bytes, 300);
        assert_eq!(diag1.scalar_bytes, 50);
        assert_eq!(diag1.prefetch_count, 1);
        assert_eq!(diag1.level_used, Some(SimdLevel::Avx512));
    }

    #[test]
    fn test_cache_stats() {
        let mut stats = CacheStats::new();
        stats.record_read(128);
        stats.record_write(64);
        stats.record_prefetch();

        assert_eq!(stats.cache_lines_read, 2);
        assert_eq!(stats.cache_lines_written, 1);
        assert_eq!(stats.prefetch_issued, 1);
        assert_eq!(stats.total_cache_lines(), 3);
    }

    #[test]
    fn test_validation_result_merge() {
        let mut res1 = ValidationResult::new();
        res1.valid_count = 10;
        res1.invalid_count = 2;
        res1.error_offsets = vec![100, 200];
        res1.bytes_validated = 1000;

        let mut res2 = ValidationResult::new();
        res2.valid_count = 15;
        res2.invalid_count = 1;
        res2.error_offsets = vec![300];
        res2.bytes_validated = 1500;

        res1.merge(&res2);

        assert_eq!(res1.valid_count, 25);
        assert_eq!(res1.invalid_count, 3);
        assert_eq!(res1.error_offsets, vec![100, 200, 300]);
        assert_eq!(res1.bytes_validated, 2500);
    }
}
