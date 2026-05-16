use serde::{Deserialize, Serialize};

/// Catálogo de venues suportadas.
///
/// Cada variante corresponde a um descriptor YAML em `venue-specs/` que
/// declara capabilities, channels, rate limits, order types e timestamps.
/// Adicionar uma variante exige descriptor versionado e teste de contrato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Venue {
    /// Conector sintético determinístico para tests / replays / benchmarks.
    Fixture,
    /// Binance Spot.
    BinanceSpot,
    /// Binance USDT-Margined Futures.
    BinanceUsdtFutures,
    /// Binance Coin-Margined (inverse) Futures.
    BinanceCoinFutures,
    /// Bybit Linear Perp/Futures.
    BybitLinear,
    /// Bybit Inverse Perp/Futures.
    BybitInverse,
    /// OKX Perp.
    OkxPerp,
    /// OKX Spot.
    OkxSpot,
    /// Coinbase Advanced.
    CoinbaseAdvanced,
    /// Kraken Pro Spot.
    KrakenPro,
    /// Kraken Futures.
    KrakenFutures,
    /// Bitget USDT-M Futures.
    BitgetFutures,
    /// Deribit options/futures.
    Deribit,
    /// Hyperliquid Perp (DEX híbrido com L3 nativo).
    HyperliquidPerp,
    /// dYdX v4 (Cosmos).
    DydxV4,
    /// GMX v2 (Arbitrum/Avalanche AMM perp).
    GmxV2,
    /// Vertex Edge (Arbitrum hybrid).
    VertexEdge,
    /// Drift v2 (Solana).
    DriftV2,
}

impl Venue {
    /// Nome humano canônico.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixture => "fixture",
            Self::BinanceSpot => "binance_spot",
            Self::BinanceUsdtFutures => "binance_usdt_futures",
            Self::BinanceCoinFutures => "binance_coin_futures",
            Self::BybitLinear => "bybit_linear",
            Self::BybitInverse => "bybit_inverse",
            Self::OkxPerp => "okx_perp",
            Self::OkxSpot => "okx_spot",
            Self::CoinbaseAdvanced => "coinbase_advanced",
            Self::KrakenPro => "kraken_pro",
            Self::KrakenFutures => "kraken_futures",
            Self::BitgetFutures => "bitget_futures",
            Self::Deribit => "deribit",
            Self::HyperliquidPerp => "hyperliquid_perp",
            Self::DydxV4 => "dydx_v4",
            Self::GmxV2 => "gmx_v2",
            Self::VertexEdge => "vertex_edge",
            Self::DriftV2 => "drift_v2",
        }
    }

    /// Indica se a venue é DEX (settlement on-chain) vs CEX.
    pub fn is_dex(self) -> bool {
        matches!(
            self,
            Self::HyperliquidPerp | Self::DydxV4 | Self::GmxV2 | Self::VertexEdge | Self::DriftV2
        )
    }
}

/// Estado operacional reportado pela venue (ou inferido por nós).
///
/// Anexado a cada `CanonicalEvent` para que consumers possam degradar
/// decisões sem precisar consultar uma fonte separada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueStatus {
    /// Operação normal.
    Normal,
    /// Latência ou drop rate elevado, mas serviço útil.
    Degraded,
    /// Manutenção anunciada.
    Maintenance,
    /// Trading halted (instrument-level ou venue-wide).
    Halted,
    /// Conectividade nossa perdida; estado desconhecido.
    Unknown,
}

impl Default for VenueStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_serde_roundtrip() {
        let v = Venue::HyperliquidPerp;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, "\"hyperliquid_perp\"");
        let back: Venue = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn dex_classification() {
        assert!(Venue::HyperliquidPerp.is_dex());
        assert!(Venue::GmxV2.is_dex());
        assert!(!Venue::BinanceSpot.is_dex());
        assert!(!Venue::BybitLinear.is_dex());
    }

    #[test]
    fn ordering_is_stable() {
        let mut v = vec![Venue::BinanceSpot, Venue::Fixture, Venue::BybitLinear];
        v.sort();
        assert_eq!(v[0], Venue::Fixture);
    }
}
