//! Capability descriptors por venue + sistema de confidence.
//!
//! Cada venue declara o que entrega (L2 vs L3, channels, latência, rate
//! limits, order types). Toda métrica derivada herda **capabilities mínimas
//! dos inputs** — se uma das venues é só L2, métricas que precisam L3 são
//! marcadas como `Low` ou `Medium` confidence (com explicação).
//!
//! Ver `docs/architecture/adr/ADR-0001` (capability-aware) e spec v2 §5.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use argus_core_types::Venue;

/// Profundidade de market data ofertada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BookDepth {
    /// Apenas best bid/ask (BBO).
    Bbo,
    /// L2 top-N (snapshot periódico).
    L2TopN(u16),
    /// L2 incremental diff (deltas + snapshots).
    L2Diff,
    /// L3 incremental (cada ordem individualmente).
    L3,
}

impl BookDepth {
    /// Capability mínima entre `self` e `other`.
    pub fn min(self, other: Self) -> Self {
        match (self, other) {
            (Self::Bbo, _) | (_, Self::Bbo) => Self::Bbo,
            (Self::L2TopN(a), Self::L2TopN(b)) => Self::L2TopN(a.min(b)),
            (Self::L2TopN(n), Self::L2Diff) | (Self::L2Diff, Self::L2TopN(n)) => Self::L2TopN(n),
            (Self::L2TopN(n), Self::L3) | (Self::L3, Self::L2TopN(n)) => Self::L2TopN(n),
            (Self::L2Diff, _) | (_, Self::L2Diff) => Self::L2Diff,
            (Self::L3, Self::L3) => Self::L3,
        }
    }

    /// Indica se inclui informação suficiente para reconstruir orderbook.
    pub fn supports_full_book(self) -> bool {
        matches!(self, Self::L2Diff | Self::L3)
    }
}

/// Confidence de uma métrica/sinal/dado.
///
/// Composite com componentes explícitos. Para uso simplificado em UI, derive
/// `level()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Confidence {
    /// Score 0..=100. Calibrado contra histórico.
    pub score: u8,
}

impl Confidence {
    /// Constrói; satura em [0, 100].
    pub fn new(score: u8) -> Self {
        Self { score: score.min(100) }
    }

    /// Alta (>= 75).
    pub fn high() -> Self {
        Self { score: 90 }
    }

    /// Média (50..=74).
    pub fn medium() -> Self {
        Self { score: 60 }
    }

    /// Baixa (< 50).
    pub fn low() -> Self {
        Self { score: 30 }
    }

    /// Nível qualitativo.
    pub fn level(self) -> ConfidenceLevel {
        match self.score {
            0..=29 => ConfidenceLevel::VeryLow,
            30..=49 => ConfidenceLevel::Low,
            50..=74 => ConfidenceLevel::Medium,
            75..=89 => ConfidenceLevel::High,
            90..=100 => ConfidenceLevel::VeryHigh,
            _ => ConfidenceLevel::Unknown,
        }
    }

    /// Mínimo entre dois — herança downstream.
    pub fn min(self, other: Self) -> Self {
        Self { score: self.score.min(other.score) }
    }
}

/// Nível qualitativo para UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    /// Desconhecido (não calibrado).
    Unknown,
    /// Muito baixo (< 30).
    VeryLow,
    /// Baixo (30..49).
    Low,
    /// Médio (50..74).
    Medium,
    /// Alto (75..89).
    High,
    /// Muito alto (>= 90).
    VeryHigh,
}

/// Channel de stream público que uma venue oferece.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VenueChannel {
    /// Nome canônico (depth, trades, funding, mark, oi, liq).
    pub kind: ChannelKind,
    /// Profundidade ofertada (para depth).
    pub depth: Option<BookDepth>,
    /// Cadência típica em ms (None = event-driven).
    pub cadence_ms: SmallVec<[u16; 4]>,
    /// Modelo de sequencing (como detectar gap).
    pub sequence_model: SequenceModel,
    /// Latência mediana esperada em ms (orientativo; mensurado em runtime).
    pub latency_p50_ms_typical: u16,
}

/// Tipo de channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// Orderbook depth (deltas e/ou snapshots).
    Depth,
    /// Trades públicos.
    Trades,
    /// Best bid/ask quotes.
    BookTicker,
    /// Funding rate (perps).
    Funding,
    /// Mark price.
    MarkPrice,
    /// Index price.
    IndexPrice,
    /// Open interest.
    OpenInterest,
    /// Liquidações.
    Liquidations,
    /// Klines/candles oficiais da venue.
    Klines,
    /// User data (private; só Risk Daemon assina).
    UserData,
}

/// Modelo de sequencing para detectar gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceModel {
    /// `update_id` strictly increasing por delta.
    UpdateIdMonotonic,
    /// `update_id` range (first/last) por mensagem.
    UpdateIdRange,
    /// `seq` increasing por mensagem WS.
    SeqMonotonic,
    /// Nenhum (sem detection nativo; usar recv_ts + heurística).
    None,
}

/// Tipos de ordem suportados pela venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OrderTypeCap {
    /// Limit order.
    Limit,
    /// Market order.
    Market,
    /// Stop loss (trigger market).
    StopMarket,
    /// Stop loss limit.
    StopLimit,
    /// Take profit (trigger market).
    TakeProfitMarket,
    /// Take profit limit.
    TakeProfitLimit,
    /// Trailing stop.
    TrailingStop,
    /// Post-only (maker only).
    PostOnly,
    /// Fill-or-kill.
    Fok,
    /// Immediate-or-cancel.
    Ioc,
    /// OCO (one-cancels-other).
    Oco,
    /// Conditional / bracket atomic.
    BracketAtomic,
}

/// Descritor completo de capacidades de uma venue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VenueCapability {
    /// Venue.
    pub venue: Venue,
    /// Versão do descriptor (semver-like).
    pub version: String,
    /// Canais públicos suportados.
    pub channels: SmallVec<[VenueChannel; 16]>,
    /// Tipos de ordem suportados.
    pub order_types: SmallVec<[OrderTypeCap; 16]>,
    /// Suporta reduce-only.
    pub supports_reduce_only: bool,
    /// Suporta hedge mode (positions long + short separadas).
    pub supports_hedge_mode: bool,
    /// Suporta OCO atomic server-side.
    pub supports_server_oco: bool,
    /// Suporta testnet.
    pub has_testnet: bool,
    /// Rate limit em requests por segundo (média).
    pub rest_rate_limit_per_sec: u32,
    /// Latência típica esperada em ms.
    pub typical_latency_p50_ms: u16,
    /// Suporta cancel/replace atomic (vs cancel+new).
    pub supports_atomic_replace: bool,
    /// Profundidade L3 disponível.
    pub has_l3: bool,
}

impl VenueCapability {
    /// Verifica se a venue tem o channel kind especificado.
    pub fn has_channel(&self, kind: ChannelKind) -> bool {
        self.channels.iter().any(|c| c.kind == kind)
    }

    /// Profundidade máxima disponível em depth channel.
    pub fn max_book_depth(&self) -> Option<BookDepth> {
        self.channels
            .iter()
            .filter(|c| c.kind == ChannelKind::Depth)
            .filter_map(|c| c.depth)
            .max_by_key(|d| match d {
                BookDepth::Bbo => 0u8,
                BookDepth::L2TopN(_) => 1,
                BookDepth::L2Diff => 2,
                BookDepth::L3 => 3,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_min_inherits() {
        let high = Confidence::high();
        let low = Confidence::low();
        assert_eq!(high.min(low).score, low.score);
    }

    #[test]
    fn confidence_levels() {
        assert_eq!(Confidence::new(95).level(), ConfidenceLevel::VeryHigh);
        assert_eq!(Confidence::new(80).level(), ConfidenceLevel::High);
        assert_eq!(Confidence::new(60).level(), ConfidenceLevel::Medium);
        assert_eq!(Confidence::new(40).level(), ConfidenceLevel::Low);
        assert_eq!(Confidence::new(15).level(), ConfidenceLevel::VeryLow);
    }

    #[test]
    fn book_depth_min() {
        assert_eq!(BookDepth::L3.min(BookDepth::L2Diff), BookDepth::L2Diff);
        assert_eq!(BookDepth::L2Diff.min(BookDepth::L2TopN(20)), BookDepth::L2TopN(20));
        assert_eq!(BookDepth::L2TopN(50).min(BookDepth::L2TopN(20)), BookDepth::L2TopN(20));
    }

    #[test]
    fn book_depth_supports_full() {
        assert!(BookDepth::L3.supports_full_book());
        assert!(BookDepth::L2Diff.supports_full_book());
        assert!(!BookDepth::L2TopN(10).supports_full_book());
        assert!(!BookDepth::Bbo.supports_full_book());
    }
}
