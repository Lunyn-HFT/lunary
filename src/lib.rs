#[cfg(feature = "allocator")]
mod allocator;

pub mod concurrent;
pub mod config;
pub mod error;
pub mod messages;
pub mod mmap;
pub mod parser;
pub mod simd;
pub mod zerocopy;
pub mod zerocopy_types;

mod builder;

pub use builder::{ParserBuilder, ParserConfig, ParserMode};
pub use concurrent::{
    AdaptiveBatchConfig, AdaptiveBatchMetrics, AdaptiveBatchProcessor, AdaptiveStrategy,
    AtomicStats, BatchProcessor, ConcurrentParser, ParallelParser, ParserMetrics,
    ParserStatsSnapshot, SpscParser, SpscQueue, UnifiedParser, WorkStealingParser, WorkerStats,
    WorkerStatsSnapshot,
};
pub use config::Config;
pub use error::{ParseError, ParseResult, Result};
pub use messages::{
    AddOrderMessage, AddOrderRef, AddOrderWithMpidMessage, AddOrderWithMpidRef, BrokenTradeMessage,
    BrokenTradeRef, CrossTradeMessage, CrossTradeRef, IpoQuotingPeriodMessage, IpoQuotingPeriodRef,
    LuldAuctionCollarMessage, LuldAuctionCollarRef, MarketParticipantPositionMessage,
    MarketParticipantPositionRef, Message, MessageRef, MwcbDeclineLevelMessage,
    MwcbDeclineLevelRef, MwcbStatusMessage, MwcbStatusRef, NetOrderImbalanceMessage,
    NetOrderImbalanceRef, OrderCancelMessage, OrderCancelRef, OrderDeleteMessage, OrderDeleteRef,
    OrderExecutedMessage, OrderExecutedRef, OrderExecutedWithPriceMessage,
    OrderExecutedWithPriceRef, OrderReplaceMessage, OrderReplaceRef, RegShoRestrictionMessage,
    RegShoRestrictionRef, RetailPriceImprovementMessage, RetailPriceImprovementRef,
    StockDirectoryMessage, StockDirectoryRef, StockTradingActionMessage, StockTradingActionRef,
    SystemEventMessage, SystemEventRef, ToOwnedMessage, TradeMessage, TradeRef, ZeroCopyParse,
};
pub use mmap::{ChunkedMmapParser, MmapParser};
pub use parser::{ParseStats, Parser};
pub use simd::{
    BoundaryResult, CacheStats, ParseDiagnostics, ParseDiagnosticsExt, SimdDiagnostics, SimdInfo,
    SimdLevel, ValidationError, ValidationResult, batch_read_u16_simd, batch_read_u32_simd,
    batch_read_u64_simd, batch_validate_messages_simd, compute_checksum_scalar,
    compute_checksum_simd, count_messages_fast, extract_timestamps_simd, is_avx512_available,
    is_simd_available, is_valid_message_type, scan_boundaries_auto, scan_boundaries_avx2,
    scan_boundaries_with_diagnostics, scan_message_lengths_simd, simd_info,
    validate_boundaries_simd, validate_checksum_simd, validate_message_sequence_simd,
    validate_message_stream_simd,
};
pub use zerocopy::{
    IntoOwned, MessageVisitor, Mpid, ParseMessage, Stock, ZeroCopyBatchProcessor, ZeroCopyIterator,
    ZeroCopyMessage, ZeroCopyParser,
};

pub use zerocopy_types::MessageHeaderRaw as MessageHeader;

pub mod bench;
