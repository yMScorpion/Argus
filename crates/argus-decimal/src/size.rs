use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::errors::NumericError;

/// Tamanho de tick declarado por instrumento.
///
/// Representado como `Decimal` para precisão exata em qualquer expressão
/// fracionária comum em cripto (ex: `0.00000001`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TickSize(Decimal);

impl TickSize {
    /// Constrói. Erra se valor <= 0.
    pub fn new(d: Decimal) -> Result<Self, NumericError> {
        if d <= Decimal::ZERO {
            return Err(NumericError::InvalidSize(format!("tick size must be > 0, got {d}")));
        }
        Ok(Self(d))
    }

    /// Acesso ao decimal interno.
    pub fn as_decimal(self) -> Decimal {
        self.0
    }
}

impl std::fmt::Display for TickSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Tamanho de lot declarado por instrumento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LotSize(Decimal);

impl LotSize {
    /// Constrói. Erra se valor <= 0.
    pub fn new(d: Decimal) -> Result<Self, NumericError> {
        if d <= Decimal::ZERO {
            return Err(NumericError::InvalidSize(format!("lot size must be > 0, got {d}")));
        }
        Ok(Self(d))
    }

    /// Acesso ao decimal interno.
    pub fn as_decimal(self) -> Decimal {
        self.0
    }
}

impl std::fmt::Display for LotSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn tick_size_rejects_zero_or_negative() {
        assert!(TickSize::new(dec!(0)).is_err());
        assert!(TickSize::new(dec!(-0.01)).is_err());
        assert!(TickSize::new(dec!(0.01)).is_ok());
    }

    #[test]
    fn lot_size_rejects_zero_or_negative() {
        assert!(LotSize::new(dec!(0)).is_err());
        assert!(LotSize::new(dec!(-1)).is_err());
        assert!(LotSize::new(dec!(0.001)).is_ok());
    }
}
