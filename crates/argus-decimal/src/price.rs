use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::errors::{NumericError, Rounding};
use crate::size::TickSize;

/// Preço como número inteiro de ticks (i128).
///
/// Sempre comparado em `i128`. Conversão de/para `Decimal` é explícita e
/// exige `TickSize` e `Rounding`.
///
/// # Invariantes
///
/// - Comparações `==`, `<`, `>` entre dois `PriceTicks` com **mesmo
///   tick_size** são corretas trivialmente.
/// - Comparações entre `PriceTicks` com tick_size **diferente** retornam
///   `None` em `partial_cmp`. Mesma instrument_id garante mesmo tick_size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PriceTicks {
    /// Número de ticks (pode ser negativo em contextos de spread/delta).
    ///
    /// Serializado como string para compatibilidade com formatos que não
    /// suportam i128 nativamente (notavelmente serde_json).
    #[serde(with = "crate::i128_str")]
    pub ticks: i128,
    /// Tick size do instrumento ao qual este preço se refere.
    pub tick_size: TickSize,
}

impl PriceTicks {
    /// Constrói a partir de ticks crus e tick size.
    pub fn from_ticks(ticks: i128, tick_size: TickSize) -> Self {
        Self { ticks, tick_size }
    }

    /// Converte de `Decimal` para `PriceTicks` aplicando `Rounding`.
    ///
    /// # Erros
    ///
    /// - `NotAligned` se `Rounding::RejectIfNotAligned` e valor não está
    ///   exatamente em um tick.
    /// - `Overflow` se o resultado não cabe em i128.
    pub fn from_decimal(value: Decimal, tick_size: TickSize, rounding: Rounding) -> Result<Self, NumericError> {
        let ts = tick_size.as_decimal();
        let div = value
            .checked_div(ts)
            .ok_or(NumericError::DivByZero)?;
        let ticks_decimal = match rounding {
            Rounding::Down => div.floor(),
            Rounding::Up => div.ceil(),
            Rounding::NearestTowardZero => {
                // Half-to-zero: arredonda 0.5 em direção a zero (mais
                // conservador para preços de execução).
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
                        tick_size: ts.to_string(),
                    });
                }
                div
            }
        };
        let ticks = ticks_decimal.to_i128().ok_or(NumericError::Overflow {
            value: value.to_string(),
            scale: ts.scale(),
        })?;
        Ok(Self { ticks, tick_size })
    }

    /// Reconstrói `Decimal` exato (sem perda).
    pub fn to_decimal(self) -> Decimal {
        // ticks × tick_size. ticks pode ser negativo.
        let sign = if self.ticks < 0 { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
        let abs_ticks = self.ticks.unsigned_abs();
        // Decimal não tem from_u128 direto; passa por string em caminho
        // raro (overflow > i64). Para a maioria absoluta de casos cabe em
        // i64.
        let dec_ticks = if abs_ticks <= i64::MAX as u128 {
            Decimal::from(abs_ticks as i64)
        } else {
            Decimal::from_str_exact(&abs_ticks.to_string()).expect("u128 stringification → Decimal")
        };
        sign * dec_ticks * self.tick_size.as_decimal()
    }

    /// Tick size associado.
    pub fn tick_size(self) -> TickSize {
        self.tick_size
    }

    /// Adição (mesmo tick_size obrigatório).
    pub fn checked_add(self, other: Self) -> Result<Self, NumericError> {
        if self.tick_size != other.tick_size {
            return Err(NumericError::InvalidSize(format!(
                "mixed tick sizes in add: {} vs {}",
                self.tick_size, other.tick_size
            )));
        }
        let ticks = self.ticks.checked_add(other.ticks).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { ticks, tick_size: self.tick_size })
    }

    /// Subtração (mesmo tick_size obrigatório).
    pub fn checked_sub(self, other: Self) -> Result<Self, NumericError> {
        if self.tick_size != other.tick_size {
            return Err(NumericError::InvalidSize(format!(
                "mixed tick sizes in sub: {} vs {}",
                self.tick_size, other.tick_size
            )));
        }
        let ticks = self.ticks.checked_sub(other.ticks).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { ticks, tick_size: self.tick_size })
    }
}

impl PartialOrd for PriceTicks {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.tick_size == other.tick_size {
            Some(self.ticks.cmp(&other.ticks))
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

    fn ts_001() -> TickSize {
        TickSize::new(dec!(0.01)).unwrap()
    }

    #[test]
    fn aligned_value_roundtrips() {
        let ts = ts_001();
        let p = PriceTicks::from_decimal(dec!(12345.67), ts, Rounding::RejectIfNotAligned).unwrap();
        assert_eq!(p.ticks, 1_234_567);
        assert_eq!(p.to_decimal(), dec!(12345.67));
    }

    #[test]
    fn reject_if_not_aligned_works() {
        let ts = ts_001();
        let r = PriceTicks::from_decimal(dec!(12345.678), ts, Rounding::RejectIfNotAligned);
        assert!(matches!(r, Err(NumericError::NotAligned { .. })));
    }

    #[test]
    fn round_down_truncates_toward_negative() {
        let ts = ts_001();
        let p = PriceTicks::from_decimal(dec!(12345.678), ts, Rounding::Down).unwrap();
        assert_eq!(p.to_decimal(), dec!(12345.67));

        let p_neg = PriceTicks::from_decimal(dec!(-12345.678), ts, Rounding::Down).unwrap();
        // floor(-12345.678 / 0.01) = floor(-1234567.8) = -1234568.
        assert_eq!(p_neg.to_decimal(), dec!(-12345.68));
    }

    #[test]
    fn round_up_truncates_toward_positive() {
        let ts = ts_001();
        let p = PriceTicks::from_decimal(dec!(12345.671), ts, Rounding::Up).unwrap();
        assert_eq!(p.to_decimal(), dec!(12345.68));
    }

    #[test]
    fn round_nearest_breaks_ties_to_zero() {
        let ts = ts_001();
        // 12345.675 / 0.01 = 1234567.5 → half exact → toward zero → 1234567.
        let p = PriceTicks::from_decimal(dec!(12345.675), ts, Rounding::NearestTowardZero).unwrap();
        assert_eq!(p.to_decimal(), dec!(12345.67));
    }

    #[test]
    fn arithmetic_requires_same_tick_size() {
        let a = PriceTicks::from_ticks(100, ts_001());
        let b = PriceTicks::from_ticks(50, TickSize::new(dec!(0.001)).unwrap());
        assert!(a.checked_add(b).is_err());
        assert!(a.partial_cmp(&b).is_none());
    }

    #[test]
    fn arithmetic_overflow_detected() {
        let a = PriceTicks::from_ticks(i128::MAX, ts_001());
        let b = PriceTicks::from_ticks(1, ts_001());
        assert!(matches!(a.checked_add(b), Err(NumericError::ArithmeticOverflow)));
    }

    proptest! {
        #[test]
        fn aligned_roundtrip_preserves_value(
            ticks in -1_000_000_000i128..1_000_000_000i128,
        ) {
            let ts = ts_001();
            let p = PriceTicks::from_ticks(ticks, ts);
            let d = p.to_decimal();
            let back = PriceTicks::from_decimal(d, ts, Rounding::RejectIfNotAligned).unwrap();
            prop_assert_eq!(back.ticks, ticks);
        }

        #[test]
        fn round_down_never_above_input(value_ticks in -100_000_000i128..100_000_000i128) {
            let ts = ts_001();
            let original = PriceTicks::from_ticks(value_ticks, ts).to_decimal();
            // Adiciona uma fração não alinhada para forçar rounding.
            let unaligned = original + dec!(0.003);
            let rounded = PriceTicks::from_decimal(unaligned, ts, Rounding::Down).unwrap();
            prop_assert!(rounded.to_decimal() <= unaligned);
        }

        #[test]
        fn round_up_never_below_input(value_ticks in -100_000_000i128..100_000_000i128) {
            let ts = ts_001();
            let original = PriceTicks::from_ticks(value_ticks, ts).to_decimal();
            let unaligned = original + dec!(0.003);
            let rounded = PriceTicks::from_decimal(unaligned, ts, Rounding::Up).unwrap();
            prop_assert!(rounded.to_decimal() >= unaligned);
        }
    }
}
