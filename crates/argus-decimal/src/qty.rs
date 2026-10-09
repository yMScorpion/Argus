use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::errors::{NumericError, Rounding};
use crate::size::LotSize;

/// Quantidade como número inteiro de lots (i128).
///
/// Análogo a `PriceTicks` mas para quantidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QtyLots {
    /// Número de lots (signed para deltas/CVD).
    ///
    /// Serializado como string para compatibilidade com formatos sem suporte
    /// a i128 nativo.
    #[serde(with = "crate::i128_str")]
    pub lots: i128,
    /// Lot size do instrumento.
    pub lot_size: LotSize,
}

impl QtyLots {
    /// Constrói a partir de lots crus.
    pub fn from_lots(lots: i128, lot_size: LotSize) -> Self {
        Self { lots, lot_size }
    }

    /// Converte de `Decimal` aplicando `Rounding`.
    pub fn from_decimal(value: Decimal, lot_size: LotSize, rounding: Rounding) -> Result<Self, NumericError> {
        let ls = lot_size.as_decimal();
        let div = value.checked_div(ls).ok_or(NumericError::DivByZero)?;
        let lots_decimal = match rounding {
            Rounding::Down => div.floor(),
            Rounding::Up => div.ceil(),
            Rounding::NearestTowardZero => {
                let truncated = div.trunc();
                let frac = div - truncated;
                let half = Decimal::ONE / Decimal::from(2);
                if frac.abs() > half {
                    if div >= Decimal::ZERO { truncated + Decimal::ONE } else { truncated - Decimal::ONE }
                } else {
                    truncated
                }
            }
            Rounding::RejectIfNotAligned => {
                if div != div.floor() {
                    return Err(NumericError::NotAligned {
                        value: value.to_string(),
                        tick_size: ls.to_string(),
                    });
                }
                div
            }
        };
        let lots = lots_decimal.to_i128().ok_or(NumericError::Overflow {
            value: value.to_string(),
            scale: ls.scale(),
        })?;
        Ok(Self { lots, lot_size })
    }

    /// Reconstrói `Decimal` exato.
    pub fn to_decimal(self) -> Decimal {
        let sign = if self.lots < 0 { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
        let abs = self.lots.unsigned_abs();
        let dec = if abs <= i64::MAX as u128 {
            Decimal::from(abs as i64)
        } else {
            Decimal::from_str_exact(&abs.to_string()).expect("u128 → Decimal")
        };
        sign * dec * self.lot_size.as_decimal()
    }

    /// Verifica se é zero.
    pub fn is_zero(self) -> bool {
        self.lots == 0
    }

    /// Soma. Mesmo lot_size obrigatório.
    pub fn checked_add(self, other: Self) -> Result<Self, NumericError> {
        if self.lot_size != other.lot_size {
            return Err(NumericError::InvalidSize(format!(
                "mixed lot sizes: {} vs {}",
                self.lot_size, other.lot_size
            )));
        }
        let lots = self.lots.checked_add(other.lots).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { lots, lot_size: self.lot_size })
    }

    /// Subtração. Mesmo lot_size obrigatório.
    pub fn checked_sub(self, other: Self) -> Result<Self, NumericError> {
        if self.lot_size != other.lot_size {
            return Err(NumericError::InvalidSize(format!(
                "mixed lot sizes: {} vs {}",
                self.lot_size, other.lot_size
            )));
        }
        let lots = self.lots.checked_sub(other.lots).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { lots, lot_size: self.lot_size })
    }
}

impl PartialOrd for QtyLots {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.lot_size == other.lot_size {
            Some(self.lots.cmp(&other.lots))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn lot_001() -> LotSize {
        LotSize::new(dec!(0.001)).unwrap()
    }

    #[test]
    fn aligned_roundtrip() {
        let l = lot_001();
        let q = QtyLots::from_decimal(dec!(1.234), l, Rounding::RejectIfNotAligned).unwrap();
        assert_eq!(q.lots, 1234);
        assert_eq!(q.to_decimal(), dec!(1.234));
    }

    #[test]
    fn negative_qty_roundtrip() {
        let l = lot_001();
        let q = QtyLots::from_lots(-1234, l);
        assert_eq!(q.to_decimal(), dec!(-1.234));
    }

    #[test]
    fn reject_unaligned_qty() {
        let l = lot_001();
        let r = QtyLots::from_decimal(dec!(1.2345), l, Rounding::RejectIfNotAligned);
        assert!(matches!(r, Err(NumericError::NotAligned { .. })));
    }

    proptest! {
        #[test]
        fn aligned_qty_roundtrip(lots in -1_000_000_000i128..1_000_000_000i128) {
            let l = lot_001();
            let q = QtyLots::from_lots(lots, l);
            let back = QtyLots::from_decimal(q.to_decimal(), l, Rounding::RejectIfNotAligned).unwrap();
            prop_assert_eq!(back.lots, lots);
        }

        #[test]
        fn add_is_commutative(a in -1_000_000i128..1_000_000i128, b in -1_000_000i128..1_000_000i128) {
            let l = lot_001();
            let qa = QtyLots::from_lots(a, l);
            let qb = QtyLots::from_lots(b, l);
            prop_assert_eq!(qa.checked_add(qb).unwrap(), qb.checked_add(qa).unwrap());
        }

        #[test]
        fn add_is_associative(
            a in -100_000i128..100_000i128,
            b in -100_000i128..100_000i128,
            c in -100_000i128..100_000i128,
        ) {
            let l = lot_001();
            let qa = QtyLots::from_lots(a, l);
            let qb = QtyLots::from_lots(b, l);
            let qc = QtyLots::from_lots(c, l);
            let left = qa.checked_add(qb).unwrap().checked_add(qc).unwrap();
            let right = qa.checked_add(qb.checked_add(qc).unwrap()).unwrap();
            prop_assert_eq!(left, right);
        }
    }
}
