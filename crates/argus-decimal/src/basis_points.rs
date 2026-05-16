use serde::{Deserialize, Serialize};

use crate::errors::NumericError;
use crate::money::MoneyMinor;

/// Basis points (centésimo de percentual; 1 bp = 0.01%).
///
/// Usado para fees, funding rates, slippage. Tipicamente cabe em i32 (range
/// ~±2 milhões de bps = ±200_000% — exagerado mas seguro).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(transparent)]
pub struct BasisPoints(pub i32);

impl BasisPoints {
    /// Zero.
    pub const ZERO: Self = Self(0);

    /// Constrói a partir de inteiro de bps.
    pub const fn from_bps(bps: i32) -> Self {
        Self(bps)
    }

    /// Inteiro cru.
    pub const fn as_bps(self) -> i32 {
        self.0
    }

    /// Aplica este bps a um `MoneyMinor` (multiplica e divide por 10000).
    ///
    /// Arredonda em direção a zero (mais conservador para fees pagas).
    pub fn apply_to(self, m: MoneyMinor) -> Result<MoneyMinor, NumericError> {
        // result = m * bps / 10000
        let bps_i128 = self.0 as i128;
        let scaled = m
            .minor
            .checked_mul(bps_i128)
            .ok_or(NumericError::ArithmeticOverflow)?;
        let divided = scaled / 10_000;
        Ok(MoneyMinor::from_minor(divided, m.decimals))
    }

    /// Soma com saturação em i32.
    pub fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl std::fmt::Display for BasisPoints {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} bps", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn apply_taker_fee() {
        // 5 bps em $1000 = $0.50
        let notional = MoneyMinor::from_decimal_exact(dec!(1000), 2).unwrap();
        let fee = BasisPoints::from_bps(5).apply_to(notional).unwrap();
        assert_eq!(fee.to_decimal(), dec!(0.50));
    }

    #[test]
    fn apply_rebate_negative() {
        // -2 bps maker rebate em $1000 = -$0.20 (rebate recebido)
        let notional = MoneyMinor::from_decimal_exact(dec!(1000), 2).unwrap();
        let rebate = BasisPoints::from_bps(-2).apply_to(notional).unwrap();
        assert_eq!(rebate.to_decimal(), dec!(-0.20));
    }

    #[test]
    fn rounds_toward_zero() {
        // 1 bp em $1.01 = $0.000101 → 0 cents (round toward zero).
        let m = MoneyMinor::from_decimal_exact(dec!(1.01), 2).unwrap();
        let fee = BasisPoints::from_bps(1).apply_to(m).unwrap();
        assert_eq!(fee.minor, 0);
    }
}
