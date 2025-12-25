use crate::config::Config;
use crate::error::{ParseError, Result};
use crate::messages::*;
#[cfg(feature = "simd")]
use crate::simd::{prefetch_next_message, read_u16_be_simd};
#[cfg(feature = "simd")]
use crate::simd::{
    read_timestamp_unchecked, read_u16_unchecked, read_u32_unchecked, read_u64_unchecked,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const CACHE_LINE_SIZE: usize = 64;

macro_rules! dispatch_fn {
    ($name:ident, $variant:ident, $parse_fn:ident) => {
        #[inline(always)]
        fn $name(&self, data: &[u8]) -> Result<Message> {
            let mut pos = 0;
            Ok(Message::$variant(self.$parse_fn(data, &mut pos)?))
        }
    };
}

macro_rules! parse_common_header {
    ($self:expr, $data:expr, $pos:expr) => {
        (|| -> Result<(u16, u16, u64)> {
            let stock_locate = $self.read_u16($data, $pos)?;
            let tracking_number = $self.read_u16($data, $pos)?;
            let timestamp = $self.read_timestamp($data, $pos)?;
            Ok((stock_locate, tracking_number, timestamp))
        })()
    };
}

#[repr(C, align(64))]
pub struct Parser {
    buffer: Vec<u8>,
    position: usize,
    messages_parsed: AtomicU64,
    bytes_processed: AtomicU64,
    config: Config,
}

#[repr(C, align(64))]
#[derive(Debug, Default, Clone, Copy)]
pub struct ParseStats {
    pub messages: u64,
    pub bytes: u64,
    pub elapsed: Duration,
}

impl ParseStats {
    #[inline]
    pub fn mps(&self) -> f64 {
        if self.elapsed.as_secs_f64() > 0.0 {
            self.messages as f64 / self.elapsed.as_secs_f64()
        } else {
            0.0
        }
    }
}

impl Parser {
    #[inline]
    pub fn new() -> Self {
        Self::with_config(Config::new())
    }

    #[inline]
    pub fn with_config(config: Config) -> Self {
        Self {
            buffer: Vec::with_capacity(config.initial_capacity),
            position: 0,
            messages_parsed: AtomicU64::new(0),
            bytes_processed: AtomicU64::new(0),
            config,
        }
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let aligned_capacity = (capacity + CACHE_LINE_SIZE - 1) & !(CACHE_LINE_SIZE - 1);
        let mut config = Config::new();
        config.initial_capacity = aligned_capacity;
        Self {
            buffer: Vec::with_capacity(aligned_capacity),
            position: 0,
            messages_parsed: AtomicU64::new(0),
            bytes_processed: AtomicU64::new(0),
            config,
        }
    }

    #[inline]
    pub fn parse_batch(&mut self, out: &mut Vec<Message>, max_messages: usize) -> Result<usize> {
        let start_len = out.len();
        for _ in 0..max_messages {
            match self.parse_next()? {
                Some(m) => out.push(m),
                None => break,
            }
        }
        Ok(out.len() - start_len)
    }

    #[inline(always)]
    pub fn messages_parsed(&self) -> u64 {
        self.messages_parsed.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn reset_stats(&self) {
        self.messages_parsed.store(0, Ordering::Relaxed);
        self.bytes_processed.store(0, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn feed_data(&mut self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        let current_len = self.buffer.len();
        let new_total = current_len
            .checked_add(data.len())
            .ok_or(ParseError::BufferOverflow {
                size: usize::MAX,
                max: self.config.max_buffer_size,
            })?;

        if new_total > self.config.max_buffer_size {
            return Err(ParseError::BufferOverflow {
                size: new_total,
                max: self.config.max_buffer_size,
            });
        }

        let compact_threshold = self.config.max_buffer_size / 2;
        if current_len > compact_threshold {
            self.compact_buffer();
        }

        let required_cap = new_total;
        if self.buffer.capacity() < required_cap {
            self.buffer.reserve(required_cap - self.buffer.capacity());
        }

        #[cfg(feature = "simd")]
        {
            if data.len() >= 64 {
                prefetch_next_message(data, 0);
            }
        }

        self.buffer.extend_from_slice(data);
        self.bytes_processed
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    #[inline(never)]
    #[cold]
    fn compact_buffer(&mut self) {
        if self.position > 0 {
            self.buffer.copy_within(self.position.., 0);
            let new_len = self.buffer.len() - self.position;
            self.buffer.truncate(new_len);
            self.position = 0;
        }
    }

    #[inline(always)]
    pub fn parse_next(&mut self) -> Result<Option<Message>> {
        let buf_len = self.buffer.len();
        let mut pos = self.position;
        if pos >= buf_len || buf_len - pos < 3 {
            return Ok(None);
        }

        let remaining = &self.buffer[pos..];

        #[cfg(feature = "simd")]
        let length = read_u16_be_simd(remaining) as usize;
        #[cfg(not(feature = "simd"))]
        let length = u16::from_be_bytes([remaining[0], remaining[1]]) as usize;

        if length > self.config.max_message_size {
            return Err(ParseError::LengthMismatch {
                msg_type: remaining[2],
                declared: length,
                expected: self.config.max_message_size,
            });
        }

        let total_size = length + 2;
        if buf_len - pos < total_size {
            return Ok(None);
        }

        #[cfg(feature = "simd")]
        {
            let next_msg_offset = pos + total_size;
            if next_msg_offset + 64 < buf_len {
                prefetch_next_message(&self.buffer, next_msg_offset);
            }
        }

        let message_type = remaining[2];
        let expected = EXPECTED_LENGTHS[message_type as usize] as usize;
        if expected != 0 && expected != length {
            return Err(ParseError::LengthMismatch {
                msg_type: message_type,
                declared: length,
                expected,
            });
        }

        let message_start = pos + 3;
        let payload_len = match length.checked_sub(1) {
            Some(v) => v,
            None => {
                return Err(ParseError::InvalidHeader {
                    reason: "message length must be at least 1 byte for message type",
                });
            }
        };
        let message_end = message_start + payload_len;
        let message_data = &self.buffer[message_start..message_end];

        let message = if let Some(func) = DISPATCH[message_type as usize] {
            func(self, message_data)?
        } else {
            return Err(ParseError::InvalidMessageType(message_type));
        };

        pos += total_size;
        self.position = pos;
        self.messages_parsed.fetch_add(1, Ordering::Relaxed);
        Ok(Some(message))
    }

    pub fn parse(&mut self, buf: &[u8]) -> Result<ParseStats> {
        let start = Instant::now();
        self.feed_data(buf)?;
        let mut messages: u64 = 0;

        while (self.parse_next()?).is_some() {
            messages += 1;
        }

        Ok(ParseStats {
            messages,
            bytes: buf.len() as u64,
            elapsed: start.elapsed(),
        })
    }

    pub fn parse_all(&mut self, buf: &[u8]) -> Result<impl Iterator<Item = Result<Message>> + '_> {
        self.feed_data(buf)?;
        Ok(std::iter::from_fn(move || match self.parse_next() {
            Ok(Some(msg)) => Some(Ok(msg)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }))
    }

    dispatch_fn!(dispatch_system_event, SystemEvent, parse_system_event);
    dispatch_fn!(
        dispatch_stock_directory,
        StockDirectory,
        parse_stock_directory
    );
    dispatch_fn!(
        dispatch_stock_trading_action,
        StockTradingAction,
        parse_stock_trading_action
    );
    dispatch_fn!(
        dispatch_reg_sho,
        RegShoRestriction,
        parse_reg_sho_restriction
    );
    dispatch_fn!(
        dispatch_mpp,
        MarketParticipantPosition,
        parse_market_participant_position
    );
    dispatch_fn!(
        dispatch_mwcb_decline,
        MwcbDeclineLevel,
        parse_mwcb_decline_level
    );
    dispatch_fn!(dispatch_mwcb_status, MwcbStatus, parse_mwcb_status);
    dispatch_fn!(dispatch_ipo, IpoQuotingPeriod, parse_ipo_quoting_period);
    dispatch_fn!(dispatch_add_order, AddOrder, parse_add_order);
    dispatch_fn!(
        dispatch_add_order_mpid,
        AddOrderWithMpid,
        parse_add_order_with_mpid
    );
    dispatch_fn!(dispatch_order_exec, OrderExecuted, parse_order_executed);
    dispatch_fn!(
        dispatch_order_exec_price,
        OrderExecutedWithPrice,
        parse_order_executed_with_price
    );
    dispatch_fn!(dispatch_order_cancel, OrderCancel, parse_order_cancel);
    dispatch_fn!(dispatch_order_delete, OrderDelete, parse_order_delete);
    dispatch_fn!(dispatch_order_replace, OrderReplace, parse_order_replace);
    dispatch_fn!(dispatch_trade, Trade, parse_trade);
    dispatch_fn!(dispatch_cross_trade, CrossTrade, parse_cross_trade);
    dispatch_fn!(dispatch_broken_trade, BrokenTrade, parse_broken_trade);
    dispatch_fn!(dispatch_noii, NetOrderImbalance, parse_net_order_imbalance);
    dispatch_fn!(
        dispatch_rpi,
        RetailPriceImprovement,
        parse_retail_price_improvement
    );
    dispatch_fn!(
        dispatch_luld_auction_collar,
        LuldAuctionCollar,
        parse_luld_auction_collar
    );
    dispatch_fn!(dispatch_direct_listing, DirectListing, parse_direct_listing);

    #[inline(always)]
    fn read_u16(&self, data: &[u8], pos: &mut usize) -> Result<u16> {
        if *pos + 2 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *pos + 2,
                actual: data.len(),
            });
        }
        #[cfg(feature = "simd")]
        let value = unsafe { read_u16_unchecked(data, *pos) };
        #[cfg(not(feature = "simd"))]
        let value = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
        *pos += 2;
        Ok(value)
    }

    #[inline(always)]
    fn read_u32(&self, data: &[u8], pos: &mut usize) -> Result<u32> {
        if *pos + 4 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *pos + 4,
                actual: data.len(),
            });
        }
        #[cfg(feature = "simd")]
        let value = unsafe { read_u32_unchecked(data, *pos) };
        #[cfg(not(feature = "simd"))]
        let value =
            u32::from_be_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
        *pos += 4;
        Ok(value)
    }

    #[inline(always)]
    fn read_u64(&self, data: &[u8], pos: &mut usize) -> Result<u64> {
        if *pos + 8 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *pos + 8,
                actual: data.len(),
            });
        }
        #[cfg(feature = "simd")]
        let value = unsafe { read_u64_unchecked(data, *pos) };
        #[cfg(not(feature = "simd"))]
        let value = u64::from_be_bytes([
            data[*pos],
            data[*pos + 1],
            data[*pos + 2],
            data[*pos + 3],
            data[*pos + 4],
            data[*pos + 5],
            data[*pos + 6],
            data[*pos + 7],
        ]);
        *pos += 8;
        Ok(value)
    }

    #[inline(always)]
    fn read_u8(&self, data: &[u8], pos: &mut usize) -> Result<u8> {
        if *pos >= data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *pos + 1,
                actual: data.len(),
            });
        }
        let value = data[*pos];
        *pos += 1;
        Ok(value)
    }

    #[inline(always)]
    fn read_stock(&self, data: &[u8], offset: &mut usize) -> Result<[u8; 8]> {
        if *offset + 8 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *offset + 8,
                actual: data.len(),
            });
        }
        let mut stock = [0u8; 8];
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().add(*offset), stock.as_mut_ptr(), 8);
        }
        *offset += 8;
        Ok(stock)
    }

    #[inline(always)]
    fn read_mpid(&self, data: &[u8], offset: &mut usize) -> Result<[u8; 4]> {
        if *offset + 4 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *offset + 4,
                actual: data.len(),
            });
        }
        let mut mpid = [0u8; 4];
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().add(*offset), mpid.as_mut_ptr(), 4);
        }
        *offset += 4;
        Ok(mpid)
    }

    #[inline(always)]
    fn read_reason(&self, data: &[u8], offset: &mut usize) -> Result<[u8; 4]> {
        if *offset + 4 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *offset + 4,
                actual: data.len(),
            });
        }
        let mut reason = [0u8; 4];
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().add(*offset), reason.as_mut_ptr(), 4);
        }
        *offset += 4;
        Ok(reason)
    }

    #[inline(always)]
    fn read_issue_sub_type(&self, data: &[u8], offset: &mut usize) -> Result<[u8; 2]> {
        if *offset + 2 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *offset + 2,
                actual: data.len(),
            });
        }
        let mut sub_type = [0u8; 2];
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().add(*offset), sub_type.as_mut_ptr(), 2);
        }
        *offset += 2;
        Ok(sub_type)
    }

    #[inline(always)]
    fn read_timestamp(&self, data: &[u8], pos: &mut usize) -> Result<u64> {
        if *pos + 6 > data.len() {
            return Err(ParseError::TruncatedMessage {
                expected: *pos + 6,
                actual: data.len(),
            });
        }
        #[cfg(feature = "simd")]
        let value = unsafe { read_timestamp_unchecked(data, *pos) };
        #[cfg(not(feature = "simd"))]
        let value = u64::from_be_bytes([
            0,
            0,
            data[*pos],
            data[*pos + 1],
            data[*pos + 2],
            data[*pos + 3],
            data[*pos + 4],
            data[*pos + 5],
        ]);
        *pos += 6;
        Ok(value)
    }

    #[inline]
    fn parse_system_event(&self, data: &[u8], pos: &mut usize) -> Result<SystemEventMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(SystemEventMessage {
            stock_locate,
            tracking_number,
            timestamp,
            event_code: self.read_u8(data, pos)? as char,
        })
    }

    #[inline]
    fn parse_stock_directory(&self, data: &[u8], pos: &mut usize) -> Result<StockDirectoryMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let market_category = self.read_u8(data, pos)? as char;
        let financial_status_indicator = self.read_u8(data, pos)? as char;

        Ok(StockDirectoryMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            market_category,
            financial_status_indicator,
            round_lot_size: self.read_u32(data, pos)?,
            round_lots_only: self.read_u8(data, pos)? as char,
            issue_classification: self.read_u8(data, pos)? as char,
            issue_sub_type: self.read_issue_sub_type(data, pos)?,
            authenticity: self.read_u8(data, pos)? as char,
            short_sale_threshold_indicator: self.read_u8(data, pos)? as char,
            ipo_flag: self.read_u8(data, pos)? as char,
            luld_reference_price_tier: self.read_u8(data, pos)? as char,
            etp_flag: self.read_u8(data, pos)? as char,
            etp_leverage_factor: self.read_u32(data, pos)?,
            inverse_indicator: self.read_u8(data, pos)? as char,
        })
    }

    #[inline]
    fn parse_stock_trading_action(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<StockTradingActionMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let trading_state = self.read_u8(data, pos)? as char;
        let reserved = self.read_u8(data, pos)? as char;
        let reason = self.read_reason(data, pos)?;

        Ok(StockTradingActionMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            trading_state,
            reserved,
            reason,
        })
    }

    #[inline]
    fn parse_reg_sho_restriction(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<RegShoRestrictionMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let reg_sho_action = self.read_u8(data, pos)? as char;

        Ok(RegShoRestrictionMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            reg_sho_action,
        })
    }

    #[inline]
    fn parse_market_participant_position(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<MarketParticipantPositionMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let mpid = self.read_mpid(data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let primary_market_maker = self.read_u8(data, pos)? as char;
        let market_maker_mode = self.read_u8(data, pos)? as char;
        let market_participant_state = self.read_u8(data, pos)? as char;

        Ok(MarketParticipantPositionMessage {
            stock_locate,
            tracking_number,
            timestamp,
            mpid,
            stock,
            primary_market_maker,
            market_maker_mode,
            market_participant_state,
        })
    }

    #[inline]
    fn parse_mwcb_decline_level(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<MwcbDeclineLevelMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(MwcbDeclineLevelMessage {
            stock_locate,
            tracking_number,
            timestamp,
            level1: self.read_u64(data, pos)?,
            level2: self.read_u64(data, pos)?,
            level3: self.read_u64(data, pos)?,
        })
    }

    #[inline]
    fn parse_mwcb_status(&self, data: &[u8], pos: &mut usize) -> Result<MwcbStatusMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(MwcbStatusMessage {
            stock_locate,
            tracking_number,
            timestamp,
            breached_level: self.read_u8(data, pos)? as char,
        })
    }

    #[inline]
    fn parse_ipo_quoting_period(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<IpoQuotingPeriodMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let stock = self.read_stock(data, pos)?;

        Ok(IpoQuotingPeriodMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            ipo_quotation_release_time: self.read_u32(data, pos)?,
            ipo_quotation_release_qualifier: self.read_u8(data, pos)? as char,
            ipo_price: self.read_u32(data, pos)?,
        })
    }

    #[inline]
    fn parse_add_order(&self, data: &[u8], pos: &mut usize) -> Result<AddOrderMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let order_reference_number = self.read_u64(data, pos)?;
        let buy_sell_indicator = self.read_u8(data, pos)? as char;
        let shares = self.read_u32(data, pos)?;
        let stock = self.read_stock(data, pos)?;

        Ok(AddOrderMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            buy_sell_indicator,
            shares,
            stock,
            price: self.read_u32(data, pos)?,
        })
    }

    #[inline]
    fn parse_add_order_with_mpid(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<AddOrderWithMpidMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let order_reference_number = self.read_u64(data, pos)?;
        let buy_sell_indicator = self.read_u8(data, pos)? as char;
        let shares = self.read_u32(data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let price = self.read_u32(data, pos)?;
        let attribution = self.read_mpid(data, pos)?;

        Ok(AddOrderWithMpidMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            buy_sell_indicator,
            shares,
            stock,
            price,
            attribution,
        })
    }

    #[inline]
    fn parse_order_executed(&self, data: &[u8], pos: &mut usize) -> Result<OrderExecutedMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(OrderExecutedMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number: self.read_u64(data, pos)?,
            executed_shares: self.read_u32(data, pos)?,
            match_number: self.read_u64(data, pos)?,
        })
    }

    #[inline]
    fn parse_order_executed_with_price(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<OrderExecutedWithPriceMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(OrderExecutedWithPriceMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number: self.read_u64(data, pos)?,
            executed_shares: self.read_u32(data, pos)?,
            match_number: self.read_u64(data, pos)?,
            printable: self.read_u8(data, pos)? as char,
            execution_price: self.read_u32(data, pos)?,
        })
    }

    #[inline]
    fn parse_order_cancel(&self, data: &[u8], pos: &mut usize) -> Result<OrderCancelMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(OrderCancelMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number: self.read_u64(data, pos)?,
            cancelled_shares: self.read_u32(data, pos)?,
        })
    }

    #[inline]
    fn parse_order_delete(&self, data: &[u8], pos: &mut usize) -> Result<OrderDeleteMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(OrderDeleteMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number: self.read_u64(data, pos)?,
        })
    }

    #[inline]
    fn parse_order_replace(&self, data: &[u8], pos: &mut usize) -> Result<OrderReplaceMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        Ok(OrderReplaceMessage {
            stock_locate,
            tracking_number,
            timestamp,
            original_order_reference_number: self.read_u64(data, pos)?,
            new_order_reference_number: self.read_u64(data, pos)?,
            shares: self.read_u32(data, pos)?,
            price: self.read_u32(data, pos)?,
        })
    }

    #[inline]
    fn parse_trade(&self, data: &[u8], pos: &mut usize) -> Result<TradeMessage> {
        let (stock_locate, tracking_number, timestamp) = parse_common_header!(self, data, pos)?;
        let order_reference_number = self.read_u64(data, pos)?;
        let buy_sell_indicator = self.read_u8(data, pos)? as char;
        let shares = self.read_u32(data, pos)?;
        let stock = self.read_stock(data, pos)?;

        Ok(TradeMessage {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            buy_sell_indicator,
            shares,
            stock,
            price: self.read_u32(data, pos)?,
            match_number: self.read_u64(data, pos)?,
        })
    }

    #[inline]
    fn parse_cross_trade(&self, data: &[u8], pos: &mut usize) -> Result<CrossTradeMessage> {
        let stock_locate = self.read_u16(data, pos)?;
        let tracking_number = self.read_u16(data, pos)?;
        let timestamp = self.read_timestamp(data, pos)?;
        let shares = self.read_u64(data, pos)?;
        let stock = self.read_stock(data, pos)?;

        Ok(CrossTradeMessage {
            stock_locate,
            tracking_number,
            timestamp,
            shares,
            stock,
            cross_price: self.read_u32(data, pos)?,
            match_number: self.read_u64(data, pos)?,
            cross_type: self.read_u8(data, pos)? as char,
        })
    }

    #[inline]
    fn parse_broken_trade(&self, data: &[u8], pos: &mut usize) -> Result<BrokenTradeMessage> {
        Ok(BrokenTradeMessage {
            stock_locate: self.read_u16(data, pos)?,
            tracking_number: self.read_u16(data, pos)?,
            timestamp: self.read_timestamp(data, pos)?,
            match_number: self.read_u64(data, pos)?,
        })
    }

    #[inline]
    fn parse_net_order_imbalance(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<NetOrderImbalanceMessage> {
        let stock_locate = self.read_u16(data, pos)?;
        let tracking_number = self.read_u16(data, pos)?;
        let timestamp = self.read_timestamp(data, pos)?;
        let paired_shares = self.read_u64(data, pos)?;
        let imbalance_shares = self.read_u64(data, pos)?;
        let imbalance_direction = self.read_u8(data, pos)? as char;
        let stock = self.read_stock(data, pos)?;

        Ok(NetOrderImbalanceMessage {
            stock_locate,
            tracking_number,
            timestamp,
            paired_shares,
            imbalance_shares,
            imbalance_direction,
            stock,
            far_price: self.read_u32(data, pos)?,
            near_price: self.read_u32(data, pos)?,
            current_reference_price: self.read_u32(data, pos)?,
            cross_type: self.read_u8(data, pos)? as char,
            price_variation_indicator: self.read_u8(data, pos)? as char,
        })
    }

    #[inline]
    fn parse_retail_price_improvement(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<RetailPriceImprovementMessage> {
        let stock_locate = self.read_u16(data, pos)?;
        let tracking_number = self.read_u16(data, pos)?;
        let timestamp = self.read_timestamp(data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let interest_flag = self.read_u8(data, pos)? as char;

        Ok(RetailPriceImprovementMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            interest_flag,
        })
    }

    #[inline]
    fn parse_luld_auction_collar(
        &self,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<LuldAuctionCollarMessage> {
        let stock_locate = self.read_u16(data, pos)?;
        let tracking_number = self.read_u16(data, pos)?;
        let timestamp = self.read_timestamp(data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let auction_collar_reference_price = self.read_u32(data, pos)?;
        let upper_auction_collar_price = self.read_u32(data, pos)?;
        let lower_auction_collar_price = self.read_u32(data, pos)?;
        let auction_collar_extension = self.read_u32(data, pos)?;

        Ok(LuldAuctionCollarMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            auction_collar_reference_price,
            upper_auction_collar_price,
            lower_auction_collar_price,
            auction_collar_extension,
        })
    }

    fn parse_direct_listing(&self, data: &[u8], pos: &mut usize) -> Result<DirectListingMessage> {
        let stock_locate = self.read_u16(data, pos)?;
        let tracking_number = self.read_u16(data, pos)?;
        let timestamp = self.read_timestamp(data, pos)?;
        let stock = self.read_stock(data, pos)?;
        let reference_price = self.read_u32(data, pos)?;
        let indicative_price = self.read_u32(data, pos)?;
        let reserve_shares = self.read_u32(data, pos)?;
        let reserve_price = self.read_u32(data, pos)?;

        Ok(DirectListingMessage {
            stock_locate,
            tracking_number,
            timestamp,
            stock,
            reference_price,
            indicative_price,
            reserve_shares,
            reserve_price,
        })
    }

    #[inline]
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.position = 0;
    }
}

const EXPECTED_LENGTHS: [u16; 256] = {
    let mut arr = [0u16; 256];
    arr[b'S' as usize] = 12;
    arr[b'R' as usize] = 39;
    arr[b'H' as usize] = 25;
    arr[b'Y' as usize] = 20;
    arr[b'L' as usize] = 26;
    arr[b'V' as usize] = 35;
    arr[b'W' as usize] = 12;
    arr[b'K' as usize] = 28;
    arr[b'A' as usize] = 36;
    arr[b'F' as usize] = 40;
    arr[b'E' as usize] = 31;
    arr[b'C' as usize] = 36;
    arr[b'X' as usize] = 23;
    arr[b'D' as usize] = 19;
    arr[b'U' as usize] = 35;
    arr[b'P' as usize] = 44;
    arr[b'Q' as usize] = 40;
    arr[b'B' as usize] = 19;
    arr[b'I' as usize] = 50;
    arr[b'N' as usize] = 20;
    arr[b'J' as usize] = 35;
    arr[b'O' as usize] = 35;
    arr
};

type DispatchEntry = Option<fn(&Parser, &[u8]) -> Result<Message>>;

static DISPATCH: [DispatchEntry; 256] = {
    let mut tbl: [DispatchEntry; 256] = [None; 256];
    tbl[b'S' as usize] = Some(Parser::dispatch_system_event);
    tbl[b'R' as usize] = Some(Parser::dispatch_stock_directory);
    tbl[b'H' as usize] = Some(Parser::dispatch_stock_trading_action);
    tbl[b'Y' as usize] = Some(Parser::dispatch_reg_sho);
    tbl[b'L' as usize] = Some(Parser::dispatch_mpp);
    tbl[b'V' as usize] = Some(Parser::dispatch_mwcb_decline);
    tbl[b'W' as usize] = Some(Parser::dispatch_mwcb_status);
    tbl[b'K' as usize] = Some(Parser::dispatch_ipo);
    tbl[b'A' as usize] = Some(Parser::dispatch_add_order);
    tbl[b'F' as usize] = Some(Parser::dispatch_add_order_mpid);
    tbl[b'E' as usize] = Some(Parser::dispatch_order_exec);
    tbl[b'C' as usize] = Some(Parser::dispatch_order_exec_price);
    tbl[b'X' as usize] = Some(Parser::dispatch_order_cancel);
    tbl[b'D' as usize] = Some(Parser::dispatch_order_delete);
    tbl[b'U' as usize] = Some(Parser::dispatch_order_replace);
    tbl[b'P' as usize] = Some(Parser::dispatch_trade);
    tbl[b'Q' as usize] = Some(Parser::dispatch_cross_trade);
    tbl[b'B' as usize] = Some(Parser::dispatch_broken_trade);
    tbl[b'I' as usize] = Some(Parser::dispatch_noii);
    tbl[b'N' as usize] = Some(Parser::dispatch_rpi);
    tbl[b'J' as usize] = Some(Parser::dispatch_luld_auction_collar);
    tbl[b'O' as usize] = Some(Parser::dispatch_direct_listing);
    tbl
};

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}
