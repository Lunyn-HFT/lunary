use crate::error::ParseError;
use std::marker::PhantomData;

#[inline(always)]
fn ascii_to_str<'a>(data: &'a [u8], field: &'static str) -> Result<&'a str, ParseError> {
    let end = data.iter().position(|&b| b == b' ').unwrap_or(data.len());
    std::str::from_utf8(&data[..end]).map_err(|_| ParseError::InvalidUtf8 { field })
}

pub trait ToOwnedMessage {
    type Owned;
    fn to_owned_message(&self) -> Self::Owned;
}

pub trait ZeroCopyParse<'a>: Sized {
    fn parse_zerocopy(data: &'a [u8]) -> Option<Self>;
    fn message_type() -> u8;
    fn expected_len() -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    SystemEvent(SystemEventMessage),
    StockDirectory(StockDirectoryMessage),
    StockTradingAction(StockTradingActionMessage),
    RegShoRestriction(RegShoRestrictionMessage),
    MarketParticipantPosition(MarketParticipantPositionMessage),
    MwcbDeclineLevel(MwcbDeclineLevelMessage),
    MwcbStatus(MwcbStatusMessage),
    IpoQuotingPeriod(IpoQuotingPeriodMessage),
    AddOrder(AddOrderMessage),
    AddOrderWithMpid(AddOrderWithMpidMessage),
    OrderExecuted(OrderExecutedMessage),
    OrderExecutedWithPrice(OrderExecutedWithPriceMessage),
    OrderCancel(OrderCancelMessage),
    OrderDelete(OrderDeleteMessage),
    OrderReplace(OrderReplaceMessage),
    Trade(TradeMessage),
    CrossTrade(CrossTradeMessage),
    BrokenTrade(BrokenTradeMessage),
    NetOrderImbalance(NetOrderImbalanceMessage),
    RetailPriceImprovement(RetailPriceImprovementMessage),
    LuldAuctionCollar(LuldAuctionCollarMessage),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageRef<'a> {
    SystemEvent(SystemEventRef<'a>),
    StockDirectory(StockDirectoryRef<'a>),
    StockTradingAction(StockTradingActionRef<'a>),
    RegShoRestriction(RegShoRestrictionRef<'a>),
    MarketParticipantPosition(MarketParticipantPositionRef<'a>),
    MwcbDeclineLevel(MwcbDeclineLevelRef<'a>),
    MwcbStatus(MwcbStatusRef<'a>),
    IpoQuotingPeriod(IpoQuotingPeriodRef<'a>),
    AddOrder(AddOrderRef<'a>),
    AddOrderWithMpid(AddOrderWithMpidRef<'a>),
    OrderExecuted(OrderExecutedRef<'a>),
    OrderExecutedWithPrice(OrderExecutedWithPriceRef<'a>),
    OrderCancel(OrderCancelRef<'a>),
    OrderDelete(OrderDeleteRef<'a>),
    OrderReplace(OrderReplaceRef<'a>),
    Trade(TradeRef<'a>),
    CrossTrade(CrossTradeRef<'a>),
    BrokenTrade(BrokenTradeRef<'a>),
    NetOrderImbalance(NetOrderImbalanceRef<'a>),
    RetailPriceImprovement(RetailPriceImprovementRef<'a>),
    LuldAuctionCollar(LuldAuctionCollarRef<'a>),
}

impl<'a> ToOwnedMessage for MessageRef<'a> {
    type Owned = Message;

    #[inline]
    fn to_owned_message(&self) -> Message {
        match self {
            MessageRef::SystemEvent(r) => Message::SystemEvent(r.to_owned_message()),
            MessageRef::StockDirectory(r) => Message::StockDirectory(r.to_owned_message()),
            MessageRef::StockTradingAction(r) => Message::StockTradingAction(r.to_owned_message()),
            MessageRef::RegShoRestriction(r) => Message::RegShoRestriction(r.to_owned_message()),
            MessageRef::MarketParticipantPosition(r) => {
                Message::MarketParticipantPosition(r.to_owned_message())
            }
            MessageRef::MwcbDeclineLevel(r) => Message::MwcbDeclineLevel(r.to_owned_message()),
            MessageRef::MwcbStatus(r) => Message::MwcbStatus(r.to_owned_message()),
            MessageRef::IpoQuotingPeriod(r) => Message::IpoQuotingPeriod(r.to_owned_message()),
            MessageRef::AddOrder(r) => Message::AddOrder(r.to_owned_message()),
            MessageRef::AddOrderWithMpid(r) => Message::AddOrderWithMpid(r.to_owned_message()),
            MessageRef::OrderExecuted(r) => Message::OrderExecuted(r.to_owned_message()),
            MessageRef::OrderExecutedWithPrice(r) => {
                Message::OrderExecutedWithPrice(r.to_owned_message())
            }
            MessageRef::OrderCancel(r) => Message::OrderCancel(r.to_owned_message()),
            MessageRef::OrderDelete(r) => Message::OrderDelete(r.to_owned_message()),
            MessageRef::OrderReplace(r) => Message::OrderReplace(r.to_owned_message()),
            MessageRef::Trade(r) => Message::Trade(r.to_owned_message()),
            MessageRef::CrossTrade(r) => Message::CrossTrade(r.to_owned_message()),
            MessageRef::BrokenTrade(r) => Message::BrokenTrade(r.to_owned_message()),
            MessageRef::NetOrderImbalance(r) => Message::NetOrderImbalance(r.to_owned_message()),
            MessageRef::RetailPriceImprovement(r) => {
                Message::RetailPriceImprovement(r.to_owned_message())
            }
            MessageRef::LuldAuctionCollar(r) => Message::LuldAuctionCollar(r.to_owned_message()),
        }
    }
}

impl<'a> MessageRef<'a> {
    #[inline]
    pub fn to_owned(&self) -> Message {
        self.to_owned_message()
    }

    #[inline]
    pub fn timestamp(&self) -> u64 {
        match self {
            MessageRef::SystemEvent(r) => r.timestamp,
            MessageRef::StockDirectory(r) => r.timestamp,
            MessageRef::StockTradingAction(r) => r.timestamp,
            MessageRef::RegShoRestriction(r) => r.timestamp,
            MessageRef::MarketParticipantPosition(r) => r.timestamp,
            MessageRef::MwcbDeclineLevel(r) => r.timestamp,
            MessageRef::MwcbStatus(r) => r.timestamp,
            MessageRef::IpoQuotingPeriod(r) => r.timestamp,
            MessageRef::AddOrder(r) => r.timestamp,
            MessageRef::AddOrderWithMpid(r) => r.timestamp,
            MessageRef::OrderExecuted(r) => r.timestamp,
            MessageRef::OrderExecutedWithPrice(r) => r.timestamp,
            MessageRef::OrderCancel(r) => r.timestamp,
            MessageRef::OrderDelete(r) => r.timestamp,
            MessageRef::OrderReplace(r) => r.timestamp,
            MessageRef::Trade(r) => r.timestamp,
            MessageRef::CrossTrade(r) => r.timestamp,
            MessageRef::BrokenTrade(r) => r.timestamp,
            MessageRef::NetOrderImbalance(r) => r.timestamp,
            MessageRef::RetailPriceImprovement(r) => r.timestamp,
            MessageRef::LuldAuctionCollar(r) => r.timestamp,
        }
    }

    #[inline]
    pub fn stock_locate(&self) -> u16 {
        match self {
            MessageRef::SystemEvent(r) => r.stock_locate,
            MessageRef::StockDirectory(r) => r.stock_locate,
            MessageRef::StockTradingAction(r) => r.stock_locate,
            MessageRef::RegShoRestriction(r) => r.stock_locate,
            MessageRef::MarketParticipantPosition(r) => r.stock_locate,
            MessageRef::MwcbDeclineLevel(r) => r.stock_locate,
            MessageRef::MwcbStatus(r) => r.stock_locate,
            MessageRef::IpoQuotingPeriod(r) => r.stock_locate,
            MessageRef::AddOrder(r) => r.stock_locate,
            MessageRef::AddOrderWithMpid(r) => r.stock_locate,
            MessageRef::OrderExecuted(r) => r.stock_locate,
            MessageRef::OrderExecutedWithPrice(r) => r.stock_locate,
            MessageRef::OrderCancel(r) => r.stock_locate,
            MessageRef::OrderDelete(r) => r.stock_locate,
            MessageRef::OrderReplace(r) => r.stock_locate,
            MessageRef::Trade(r) => r.stock_locate,
            MessageRef::CrossTrade(r) => r.stock_locate,
            MessageRef::BrokenTrade(r) => r.stock_locate,
            MessageRef::NetOrderImbalance(r) => r.stock_locate,
            MessageRef::RetailPriceImprovement(r) => r.stock_locate,
            MessageRef::LuldAuctionCollar(r) => r.stock_locate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemEventRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub event_code: char,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> SystemEventRef<'a> {
    #[inline]
    pub fn new(stock_locate: u16, tracking_number: u16, timestamp: u64, event_code: char) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            event_code,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> SystemEventMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for SystemEventRef<'a> {
    type Owned = SystemEventMessage;

    #[inline]
    fn to_owned_message(&self) -> SystemEventMessage {
        SystemEventMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            event_code: self.event_code,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StockDirectoryRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: &'a [u8; 8],
    pub market_category: char,
    pub financial_status_indicator: char,
    pub round_lot_size: u32,
    pub round_lots_only: char,
    pub issue_classification: char,
    pub issue_sub_type: &'a [u8; 2],
    pub authenticity: char,
    pub short_sale_threshold_indicator: char,
    pub ipo_flag: char,
    pub luld_reference_price_tier: char,
    pub etp_flag: char,
    pub etp_leverage_factor: u32,
    pub inverse_indicator: char,
}

impl<'a> StockDirectoryRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> StockDirectoryMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for StockDirectoryRef<'a> {
    type Owned = StockDirectoryMessage;

    #[inline]
    fn to_owned_message(&self) -> StockDirectoryMessage {
        StockDirectoryMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            stock: *self.stock,
            market_category: self.market_category,
            financial_status_indicator: self.financial_status_indicator,
            round_lot_size: self.round_lot_size,
            round_lots_only: self.round_lots_only,
            issue_classification: self.issue_classification,
            issue_sub_type: *self.issue_sub_type,
            authenticity: self.authenticity,
            short_sale_threshold_indicator: self.short_sale_threshold_indicator,
            ipo_flag: self.ipo_flag,
            luld_reference_price_tier: self.luld_reference_price_tier,
            etp_flag: self.etp_flag,
            etp_leverage_factor: self.etp_leverage_factor,
            inverse_indicator: self.inverse_indicator,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StockTradingActionRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: &'a [u8; 8],
    pub trading_state: char,
    pub reserved: char,
    pub reason: &'a [u8; 4],
}

impl<'a> StockTradingActionRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> StockTradingActionMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for StockTradingActionRef<'a> {
    type Owned = StockTradingActionMessage;

    #[inline]
    fn to_owned_message(&self) -> StockTradingActionMessage {
        StockTradingActionMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            stock: *self.stock,
            trading_state: self.trading_state,
            reserved: self.reserved,
            reason: *self.reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegShoRestrictionRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: &'a [u8; 8],
    pub reg_sho_action: char,
}

impl<'a> RegShoRestrictionRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> RegShoRestrictionMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for RegShoRestrictionRef<'a> {
    type Owned = RegShoRestrictionMessage;

    #[inline]
    fn to_owned_message(&self) -> RegShoRestrictionMessage {
        RegShoRestrictionMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            stock: *self.stock,
            reg_sho_action: self.reg_sho_action,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarketParticipantPositionRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub mpid: &'a [u8; 4],
    pub stock: &'a [u8; 8],
    pub primary_market_maker: char,
    pub market_maker_mode: char,
    pub market_participant_state: char,
}

impl<'a> MarketParticipantPositionRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn mpid_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.mpid, "mpid")
    }

    #[inline]
    pub fn to_owned(&self) -> MarketParticipantPositionMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for MarketParticipantPositionRef<'a> {
    type Owned = MarketParticipantPositionMessage;

    #[inline]
    fn to_owned_message(&self) -> MarketParticipantPositionMessage {
        MarketParticipantPositionMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            mpid: *self.mpid,
            stock: *self.stock,
            primary_market_maker: self.primary_market_maker,
            market_maker_mode: self.market_maker_mode,
            market_participant_state: self.market_participant_state,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MwcbDeclineLevelRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub level1: u64,
    pub level2: u64,
    pub level3: u64,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> MwcbDeclineLevelRef<'a> {
    #[inline]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        level1: u64,
        level2: u64,
        level3: u64,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            level1,
            level2,
            level3,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> MwcbDeclineLevelMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for MwcbDeclineLevelRef<'a> {
    type Owned = MwcbDeclineLevelMessage;

    #[inline]
    fn to_owned_message(&self) -> MwcbDeclineLevelMessage {
        MwcbDeclineLevelMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            level1: self.level1,
            level2: self.level2,
            level3: self.level3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MwcbStatusRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub breached_level: char,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> MwcbStatusRef<'a> {
    #[inline]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        breached_level: char,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            breached_level,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> MwcbStatusMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for MwcbStatusRef<'a> {
    type Owned = MwcbStatusMessage;

    #[inline]
    fn to_owned_message(&self) -> MwcbStatusMessage {
        MwcbStatusMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            breached_level: self.breached_level,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IpoQuotingPeriodRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: &'a [u8; 8],
    pub ipo_quotation_release_time: u32,
    pub ipo_quotation_release_qualifier: char,
    pub ipo_price: u32,
}

impl<'a> IpoQuotingPeriodRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> IpoQuotingPeriodMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for IpoQuotingPeriodRef<'a> {
    type Owned = IpoQuotingPeriodMessage;

    #[inline]
    fn to_owned_message(&self) -> IpoQuotingPeriodMessage {
        IpoQuotingPeriodMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            stock: *self.stock,
            ipo_quotation_release_time: self.ipo_quotation_release_time,
            ipo_quotation_release_qualifier: self.ipo_quotation_release_qualifier,
            ipo_price: self.ipo_price,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AddOrderRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub buy_sell_indicator: char,
    pub shares: u32,
    pub stock: &'a [u8; 8],
    pub price: u32,
}

impl<'a> AddOrderRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> AddOrderMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for AddOrderRef<'a> {
    type Owned = AddOrderMessage;

    #[inline]
    fn to_owned_message(&self) -> AddOrderMessage {
        AddOrderMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
            buy_sell_indicator: self.buy_sell_indicator,
            shares: self.shares,
            stock: *self.stock,
            price: self.price,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AddOrderWithMpidRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub buy_sell_indicator: char,
    pub shares: u32,
    pub stock: &'a [u8; 8],
    pub price: u32,
    pub attribution: &'a [u8; 4],
}

impl<'a> AddOrderWithMpidRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn attribution_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.attribution, "attribution")
    }

    #[inline]
    pub fn to_owned(&self) -> AddOrderWithMpidMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for AddOrderWithMpidRef<'a> {
    type Owned = AddOrderWithMpidMessage;

    #[inline]
    fn to_owned_message(&self) -> AddOrderWithMpidMessage {
        AddOrderWithMpidMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
            buy_sell_indicator: self.buy_sell_indicator,
            shares: self.shares,
            stock: *self.stock,
            price: self.price,
            attribution: *self.attribution,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderExecutedRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> OrderExecutedRef<'a> {
    #[inline]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        order_reference_number: u64,
        executed_shares: u32,
        match_number: u64,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            executed_shares,
            match_number,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> OrderExecutedMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for OrderExecutedRef<'a> {
    type Owned = OrderExecutedMessage;

    #[inline]
    fn to_owned_message(&self) -> OrderExecutedMessage {
        OrderExecutedMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
            executed_shares: self.executed_shares,
            match_number: self.match_number,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderExecutedWithPriceRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
    pub printable: char,
    pub execution_price: u32,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> OrderExecutedWithPriceRef<'a> {
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        order_reference_number: u64,
        executed_shares: u32,
        match_number: u64,
        printable: char,
        execution_price: u32,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            executed_shares,
            match_number,
            printable,
            execution_price,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> OrderExecutedWithPriceMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for OrderExecutedWithPriceRef<'a> {
    type Owned = OrderExecutedWithPriceMessage;

    #[inline]
    fn to_owned_message(&self) -> OrderExecutedWithPriceMessage {
        OrderExecutedWithPriceMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
            executed_shares: self.executed_shares,
            match_number: self.match_number,
            printable: self.printable,
            execution_price: self.execution_price,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderCancelRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub cancelled_shares: u32,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> OrderCancelRef<'a> {
    #[inline]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        order_reference_number: u64,
        cancelled_shares: u32,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            cancelled_shares,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> OrderCancelMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for OrderCancelRef<'a> {
    type Owned = OrderCancelMessage;

    #[inline]
    fn to_owned_message(&self) -> OrderCancelMessage {
        OrderCancelMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
            cancelled_shares: self.cancelled_shares,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderDeleteRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> OrderDeleteRef<'a> {
    #[inline]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        order_reference_number: u64,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            order_reference_number,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> OrderDeleteMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for OrderDeleteRef<'a> {
    type Owned = OrderDeleteMessage;

    #[inline]
    fn to_owned_message(&self) -> OrderDeleteMessage {
        OrderDeleteMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderReplaceRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub original_order_reference_number: u64,
    pub new_order_reference_number: u64,
    pub shares: u32,
    pub price: u32,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> OrderReplaceRef<'a> {
    #[inline]
    pub fn new(
        stock_locate: u16,
        tracking_number: u16,
        timestamp: u64,
        original_order_reference_number: u64,
        new_order_reference_number: u64,
        shares: u32,
        price: u32,
    ) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            original_order_reference_number,
            new_order_reference_number,
            shares,
            price,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> OrderReplaceMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for OrderReplaceRef<'a> {
    type Owned = OrderReplaceMessage;

    #[inline]
    fn to_owned_message(&self) -> OrderReplaceMessage {
        OrderReplaceMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            original_order_reference_number: self.original_order_reference_number,
            new_order_reference_number: self.new_order_reference_number,
            shares: self.shares,
            price: self.price,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub buy_sell_indicator: char,
    pub shares: u32,
    pub stock: &'a [u8; 8],
    pub price: u32,
    pub match_number: u64,
}

impl<'a> TradeRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> TradeMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for TradeRef<'a> {
    type Owned = TradeMessage;

    #[inline]
    fn to_owned_message(&self) -> TradeMessage {
        TradeMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            order_reference_number: self.order_reference_number,
            buy_sell_indicator: self.buy_sell_indicator,
            shares: self.shares,
            stock: *self.stock,
            price: self.price,
            match_number: self.match_number,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossTradeRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub shares: u64,
    pub stock: &'a [u8; 8],
    pub cross_price: u32,
    pub match_number: u64,
    pub cross_type: char,
}

impl<'a> CrossTradeRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> CrossTradeMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for CrossTradeRef<'a> {
    type Owned = CrossTradeMessage;

    #[inline]
    fn to_owned_message(&self) -> CrossTradeMessage {
        CrossTradeMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            shares: self.shares,
            stock: *self.stock,
            cross_price: self.cross_price,
            match_number: self.match_number,
            cross_type: self.cross_type,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrokenTradeRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub match_number: u64,
    _marker: PhantomData<&'a [u8]>,
}

impl<'a> BrokenTradeRef<'a> {
    #[inline]
    pub fn new(stock_locate: u16, tracking_number: u16, timestamp: u64, match_number: u64) -> Self {
        Self {
            stock_locate,
            tracking_number,
            timestamp,
            match_number,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn to_owned(&self) -> BrokenTradeMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for BrokenTradeRef<'a> {
    type Owned = BrokenTradeMessage;

    #[inline]
    fn to_owned_message(&self) -> BrokenTradeMessage {
        BrokenTradeMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            match_number: self.match_number,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetOrderImbalanceRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub paired_shares: u64,
    pub imbalance_shares: u64,
    pub imbalance_direction: char,
    pub stock: &'a [u8; 8],
    pub far_price: u32,
    pub near_price: u32,
    pub current_reference_price: u32,
    pub cross_type: char,
    pub price_variation_indicator: char,
}

impl<'a> NetOrderImbalanceRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> NetOrderImbalanceMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for NetOrderImbalanceRef<'a> {
    type Owned = NetOrderImbalanceMessage;

    #[inline]
    fn to_owned_message(&self) -> NetOrderImbalanceMessage {
        NetOrderImbalanceMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            paired_shares: self.paired_shares,
            imbalance_shares: self.imbalance_shares,
            imbalance_direction: self.imbalance_direction,
            stock: *self.stock,
            far_price: self.far_price,
            near_price: self.near_price,
            current_reference_price: self.current_reference_price,
            cross_type: self.cross_type,
            price_variation_indicator: self.price_variation_indicator,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetailPriceImprovementRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: &'a [u8; 8],
    pub interest_flag: char,
}

impl<'a> RetailPriceImprovementRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> RetailPriceImprovementMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for RetailPriceImprovementRef<'a> {
    type Owned = RetailPriceImprovementMessage;

    #[inline]
    fn to_owned_message(&self) -> RetailPriceImprovementMessage {
        RetailPriceImprovementMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            stock: *self.stock,
            interest_flag: self.interest_flag,
        }
    }
}

/// LULD Auction Collar Reference (Type 'J')
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LuldAuctionCollarRef<'a> {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: &'a [u8; 8],
    pub auction_collar_reference_price: u32,
    pub upper_auction_collar_price: u32,
    pub lower_auction_collar_price: u32,
    pub auction_collar_extension: u32,
}

impl<'a> LuldAuctionCollarRef<'a> {
    #[inline]
    pub fn stock_str(&self) -> Result<&str, ParseError> {
        ascii_to_str(self.stock, "stock")
    }

    #[inline]
    pub fn to_owned(&self) -> LuldAuctionCollarMessage {
        self.to_owned_message()
    }
}

impl<'a> ToOwnedMessage for LuldAuctionCollarRef<'a> {
    type Owned = LuldAuctionCollarMessage;

    #[inline]
    fn to_owned_message(&self) -> LuldAuctionCollarMessage {
        LuldAuctionCollarMessage {
            stock_locate: self.stock_locate,
            tracking_number: self.tracking_number,
            timestamp: self.timestamp,
            stock: *self.stock,
            auction_collar_reference_price: self.auction_collar_reference_price,
            upper_auction_collar_price: self.upper_auction_collar_price,
            lower_auction_collar_price: self.lower_auction_collar_price,
            auction_collar_extension: self.auction_collar_extension,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemEventMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub event_code: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StockDirectoryMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub market_category: char,
    pub financial_status_indicator: char,
    pub round_lot_size: u32,
    pub round_lots_only: char,
    pub issue_classification: char,
    pub issue_sub_type: [u8; 2],
    pub authenticity: char,
    pub short_sale_threshold_indicator: char,
    pub ipo_flag: char,
    pub luld_reference_price_tier: char,
    pub etp_flag: char,
    pub etp_leverage_factor: u32,
    pub inverse_indicator: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StockTradingActionMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub trading_state: char,
    pub reserved: char,
    pub reason: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegShoRestrictionMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub reg_sho_action: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketParticipantPositionMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub mpid: [u8; 4],
    pub stock: [u8; 8],
    pub primary_market_maker: char,
    pub market_maker_mode: char,
    pub market_participant_state: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MwcbDeclineLevelMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub level1: u64,
    pub level2: u64,
    pub level3: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MwcbStatusMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub breached_level: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IpoQuotingPeriodMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub ipo_quotation_release_time: u32,
    pub ipo_quotation_release_qualifier: char,
    pub ipo_price: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddOrderMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub buy_sell_indicator: char,
    pub shares: u32,
    pub stock: [u8; 8],
    pub price: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddOrderWithMpidMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub buy_sell_indicator: char,
    pub shares: u32,
    pub stock: [u8; 8],
    pub price: u32,
    pub attribution: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderExecutedMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderExecutedWithPriceMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
    pub printable: char,
    pub execution_price: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderCancelMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub cancelled_shares: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderDeleteMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderReplaceMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub original_order_reference_number: u64,
    pub new_order_reference_number: u64,
    pub shares: u32,
    pub price: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub order_reference_number: u64,
    pub buy_sell_indicator: char,
    pub shares: u32,
    pub stock: [u8; 8],
    pub price: u32,
    pub match_number: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrossTradeMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub shares: u64,
    pub stock: [u8; 8],
    pub cross_price: u32,
    pub match_number: u64,
    pub cross_type: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrokenTradeMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub match_number: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetOrderImbalanceMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub paired_shares: u64,
    pub imbalance_shares: u64,
    pub imbalance_direction: char,
    pub stock: [u8; 8],
    pub far_price: u32,
    pub near_price: u32,
    pub current_reference_price: u32,
    pub cross_type: char,
    pub price_variation_indicator: char,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetailPriceImprovementMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub interest_flag: char,
}

/// LULD Auction Collar Message (Type 'J')
/// Indicates the auction collar thresholds for Limit Up/Limit Down
#[derive(Debug, Clone, PartialEq)]
pub struct LuldAuctionCollarMessage {
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub stock: [u8; 8],
    pub auction_collar_reference_price: u32,
    pub upper_auction_collar_price: u32,
    pub lower_auction_collar_price: u32,
    pub auction_collar_extension: u32,
}
