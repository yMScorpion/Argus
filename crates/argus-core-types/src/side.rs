use serde::{Deserialize, Serialize};

/// Lado de um trade ou intenção.
///
/// `Side` é deliberadamente simples — `Buy`/`Sell`. Conceitos como "abrir long"
/// vs "fechar short" são modelados em `IntentKind` no Risk Daemon, não aqui,
/// porque dependem de `position_mode` (hedge vs one-way) da venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Aggressor de compra (taker on ask) ou intenção de comprar.
    Buy,
    /// Aggressor de venda (taker on bid) ou intenção de vender.
    Sell,
}

impl Side {
    /// Lado oposto.
    pub fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }

    /// Sinal multiplicativo para deltas (+1 buy, -1 sell).
    ///
    /// Útil para acumular CVD, imbalance, signed volume.
    pub fn signed(self) -> i8 {
        match self {
            Self::Buy => 1,
            Self::Sell => -1,
        }
    }

    /// Nome canônico curto (uma letra) para logs compactos.
    pub fn short(self) -> char {
        match self {
            Self::Buy => 'B',
            Self::Sell => 'S',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_is_involution() {
        assert_eq!(Side::Buy.opposite().opposite(), Side::Buy);
        assert_eq!(Side::Sell.opposite().opposite(), Side::Sell);
    }

    #[test]
    fn signed_sums_correctly() {
        let trades = [Side::Buy, Side::Buy, Side::Sell, Side::Buy, Side::Sell];
        let cvd: i32 = trades.iter().copied().map(|s| s.signed() as i32).sum();
        assert_eq!(cvd, 1);
    }
}
