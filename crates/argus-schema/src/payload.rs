use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use argus_core_types::{Side, TradeId};
use argus_decimal::{MoneyMinor, PriceTicks, QtyLots};
use argus_time::UnixNanos;

/// Trade público.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradePayload {
    /// Preço executado.
    pub price: PriceTicks,
    /// Quantidade executada.
    pub qty: QtyLots,
    /// Lado do aggressor (ou inferido — marcado em quality flags).
    pub aggressor: Side,
    /// `trade_id` da venue.
    pub trade_id: TradeId,
    /// Indica se aggressor é maker (raro mas possível em alguns reports).
    pub aggressor_is_maker: bool,
}

/// Nível individual em snapshot/delta de book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookLevel {
    /// Preço.
    pub price: PriceTicks,
    /// Quantidade nesse nível (zero significa "remova esse nível").
    pub qty: QtyLots,
}

/// Snapshot completo (ou top-N) do orderbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookSnapshot {
    /// `update_id` ou `seq` que este snapshot representa.
    pub snapshot_id: u64,
    /// Lados bid (ordenado descending por preço).
    pub bids: SmallVec<[BookLevel; 64]>,
    /// Lados ask (ordenado ascending por preço).
    pub asks: SmallVec<[BookLevel; 64]>,
}

/// Delta de book (adicionar/atualizar/remover níveis).
///
/// `qty == 0` em um level significa **remova esse preço**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookDelta {
    /// Sequence id que este delta avança (continuação do snapshot anterior).
    pub from_seq: u64,
    /// Sequence id após aplicar (inclusive).
    pub to_seq: u64,
    /// Bids alterados.
    pub bid_changes: SmallVec<[BookLevel; 8]>,
    /// Asks alterados.
    pub ask_changes: SmallVec<[BookLevel; 8]>,
}

/// Funding rate (perps).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingPayload {
    /// Funding rate atual em basis points (sinal preservado).
    pub rate_bps: i32,
    /// Próximo pagamento de funding.
    pub next_funding_ts: UnixNanos,
    /// Intervalo de funding em segundos.
    pub funding_interval_sec: u32,
}

/// Mark price (índice de liquidation/PnL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkPricePayload {
    /// Mark price.
    pub mark: PriceTicks,
}

/// Index price (referência subjacente).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexPricePayload {
    /// Index price.
    pub index: PriceTicks,
}

/// Open interest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenInterestPayload {
    /// OI em qty.
    pub oi_qty: QtyLots,
    /// OI notional em quote.
    pub oi_notional: MoneyMinor,
}

/// Liquidation event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationPayload {
    /// Side que foi liquidado.
    pub side: Side,
    /// Preço de liquidação.
    pub price: PriceTicks,
    /// Quantidade liquidada.
    pub qty: QtyLots,
}

/// Payload concreto associado a um `CanonicalEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    /// Trade público.
    Trade(TradePayload),
    /// Snapshot do book.
    BookSnapshot(BookSnapshot),
    /// Delta do book.
    BookDelta(BookDelta),
    /// Funding.
    Funding(FundingPayload),
    /// Mark price.
    MarkPrice(MarkPricePayload),
    /// Index price.
    IndexPrice(IndexPricePayload),
    /// Open interest.
    OpenInterest(OpenInterestPayload),
    /// Liquidation.
    Liquidation(LiquidationPayload),
}

impl EventPayload {
    /// Indica se este payload deve atualizar orderbook.
    pub fn touches_book(&self) -> bool {
        matches!(self, Self::BookSnapshot(_) | Self::BookDelta(_))
    }

    /// Indica se este payload é de trade.
    pub fn is_trade(&self) -> bool {
        matches!(self, Self::Trade(_))
    }
}
