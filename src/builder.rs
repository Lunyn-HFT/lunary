use crate::concurrent::{
    AdaptiveBatchConfig, AdaptiveBatchProcessor, AdaptiveStrategy, BatchProcessor, ParallelParser,
    SpscParser, WorkStealingParser,
};
use crate::mmap::{MmapParser, MmapParserShared};
use crate::parser::Parser;
use crate::zerocopy::ZeroCopyParser;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserMode {
    Simple,
    Batch,
    Adaptive,
    Parallel,
    WorkStealing,
    Spsc,
    #[default]
    ZeroCopy,
    Mmap,
}

#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub mode: ParserMode,
    pub batch_size: usize,
    pub num_workers: usize,
    pub adaptive_strategy: AdaptiveStrategy,
    pub target_latency_us: u64,
    pub target_throughput_mps: Option<f64>,
    pub buffer_capacity: usize,
    pub enable_simd: bool,
    pub enable_prefetch: bool,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            mode: ParserMode::default(),
            batch_size: 4096,
            num_workers: std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4),
            adaptive_strategy: AdaptiveStrategy::default(),
            target_latency_us: 100,
            target_throughput_mps: None,
            buffer_capacity: 4 * 1024 * 1024,
            enable_simd: true,
            enable_prefetch: true,
        }
    }
}

pub struct ParserBuilder {
    config: ParserConfig,
}

impl ParserBuilder {
    pub fn new() -> Self {
        Self {
            config: ParserConfig::default(),
        }
    }

    pub fn mode(mut self, mode: ParserMode) -> Self {
        self.config.mode = mode;
        self
    }

    pub fn simple(mut self) -> Self {
        self.config.mode = ParserMode::Simple;
        self
    }

    pub fn batch(mut self, size: usize) -> Self {
        self.config.mode = ParserMode::Batch;
        self.config.batch_size = size.max(1);
        self
    }

    pub fn adaptive(mut self) -> Self {
        self.config.mode = ParserMode::Adaptive;
        self
    }

    pub fn parallel(mut self, workers: usize) -> Self {
        self.config.mode = ParserMode::Parallel;
        let max_workers = std::thread::available_parallelism()
            .map(|p| p.get() * 2)
            .unwrap_or(16);
        self.config.num_workers = workers.clamp(1, max_workers);
        self
    }

    pub fn work_stealing(mut self, workers: usize) -> Self {
        self.config.mode = ParserMode::WorkStealing;
        let max_workers = std::thread::available_parallelism()
            .map(|p| p.get() * 2)
            .unwrap_or(16);
        self.config.num_workers = workers.clamp(1, max_workers);
        self
    }

    pub fn spsc(mut self) -> Self {
        self.config.mode = ParserMode::Spsc;
        self
    }

    pub fn zero_copy(mut self) -> Self {
        self.config.mode = ParserMode::ZeroCopy;
        self
    }

    pub fn mmap(mut self) -> Self {
        self.config.mode = ParserMode::Mmap;
        self
    }

    pub fn with_strategy(mut self, strategy: AdaptiveStrategy) -> Self {
        self.config.adaptive_strategy = strategy;
        self
    }

    pub fn with_latency_target(mut self, us: u64) -> Self {
        self.config.target_latency_us = us;
        self
    }

    pub fn with_throughput_target(mut self, mps: f64) -> Self {
        self.config.target_throughput_mps = Some(mps);
        self
    }

    pub fn with_buffer_capacity(mut self, capacity: usize) -> Self {
        self.config.buffer_capacity = capacity;
        self
    }

    pub fn simd(mut self, enable: bool) -> Self {
        self.config.enable_simd = enable;
        self
    }

    pub fn prefetch(mut self, enable: bool) -> Self {
        self.config.enable_prefetch = enable;
        self
    }

    pub fn config(&self) -> &ParserConfig {
        &self.config
    }

    pub fn build_simple(&self) -> Parser {
        Parser::with_capacity(self.config.buffer_capacity)
    }

    pub fn build_batch(&self) -> BatchProcessor {
        BatchProcessor::new(self.config.batch_size)
    }

    pub fn build_adaptive(&self) -> AdaptiveBatchProcessor {
        let mut adaptive_config = AdaptiveBatchConfig::default()
            .with_strategy(self.config.adaptive_strategy)
            .with_latency_target(self.config.target_latency_us);

        if let Some(mps) = self.config.target_throughput_mps {
            adaptive_config = adaptive_config.with_throughput_target(mps);
        }

        AdaptiveBatchProcessor::with_config(adaptive_config)
    }

    pub fn build_parallel(&self) -> ParallelParser {
        ParallelParser::new(self.config.num_workers)
    }

    pub fn build_work_stealing(&self) -> WorkStealingParser {
        WorkStealingParser::new(self.config.num_workers)
    }

    pub fn build_spsc(&self) -> SpscParser {
        SpscParser::new()
    }

    pub fn build_zerocopy<'a>(&self, data: &'a [u8]) -> ZeroCopyParser<'a> {
        ZeroCopyParser::new(data)
    }

    pub fn build_mmap(&self, path: &std::path::Path) -> crate::Result<MmapParser> {
        Ok(MmapParser::open(path)?)
    }

    pub fn build_mmap_shared(
        &self,
        path: &std::path::Path,
    ) -> crate::Result<Arc<MmapParserShared>> {
        Ok(Arc::new(MmapParserShared::open(path)?))
    }
}

impl Default for ParserBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub enum AnyParser<'a> {
    Simple(Box<Parser>),
    Batch(Box<BatchProcessor>),
    Adaptive(Box<AdaptiveBatchProcessor>),
    Parallel(Box<ParallelParser>),
    WorkStealing(Box<WorkStealingParser>),
    Spsc(Box<SpscParser>),
    ZeroCopy(ZeroCopyParser<'a>),
    Mmap(Box<MmapParser>),
}

impl ParserBuilder {
    pub fn build_any<'a>(
        &self,
        data: Option<&'a [u8]>,
        mmap_path: Option<&std::path::Path>,
    ) -> crate::Result<AnyParser<'a>> {
        match self.config.mode {
            ParserMode::Simple => Ok(AnyParser::Simple(Box::new(self.build_simple()))),
            ParserMode::Batch => Ok(AnyParser::Batch(Box::new(self.build_batch()))),
            ParserMode::Adaptive => Ok(AnyParser::Adaptive(Box::new(self.build_adaptive()))),
            ParserMode::Parallel => Ok(AnyParser::Parallel(Box::new(self.build_parallel()))),
            ParserMode::WorkStealing => Ok(AnyParser::WorkStealing(Box::new(
                self.build_work_stealing(),
            ))),
            ParserMode::Spsc => Ok(AnyParser::Spsc(Box::new(self.build_spsc()))),
            ParserMode::ZeroCopy => {
                let d = data.ok_or_else(|| {
                    crate::ParseError::InvalidArgument(
                        "ZeroCopy mode requires data slice to be provided".to_string(),
                    )
                })?;
                Ok(AnyParser::ZeroCopy(self.build_zerocopy(d)))
            }
            ParserMode::Mmap => {
                let path = mmap_path.ok_or_else(|| {
                    crate::ParseError::InvalidArgument(
                        "Mmap mode requires a file path to be provided".to_string(),
                    )
                })?;
                Ok(AnyParser::Mmap(Box::new(self.build_mmap(path)?)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = ParserBuilder::new();
        assert_eq!(builder.config().mode, ParserMode::ZeroCopy);
    }

    #[test]
    fn test_builder_chain() {
        let builder = ParserBuilder::new()
            .adaptive()
            .with_strategy(AdaptiveStrategy::Throughput)
            .with_latency_target(50);

        assert_eq!(builder.config().mode, ParserMode::Adaptive);
        assert_eq!(
            builder.config().adaptive_strategy,
            AdaptiveStrategy::Throughput
        );
        assert_eq!(builder.config().target_latency_us, 50);
    }
}
