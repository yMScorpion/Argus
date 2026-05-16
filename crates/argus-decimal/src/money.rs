use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::errors::NumericError;

/// Valor monetário em sub-unidade da moeda (i128).
///
/// Exemplos:
/// - USD: `minor_unit = 1` cent (USD com `decimals = 2`).
/// - BTC: `minor_unit = 1` sat (BTC com `decimals = 8`).
/// - USDT: `minor_unit = 1` micro-USDT (USDT com `decimals = 6`).
///
/// `i128` cobre, em sats, mais BTC do que existem no universo. Para USD em
/// centavos, cobre PIB mundial vezes 10^14. Não há razão para upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MoneyMinor {
    /// Valor em sub-unidade.
    ///
    /// Serializado como string para compatibilidade com formatos sem suporte
    /// a i128 nativo.
    #[serde(with = "crate::i128_str")]
    pub minor: i128,
    /// Quantos decimais a moeda usa (ex: USD = 2, BTC = 8, USDT = 6).
    pub decimals: u8,
}

impl MoneyMinor {
    /// Constrói a partir de minor units crus.
    pub fn from_minor(minor: i128, decimals: u8) -> Self {
        Self { minor, decimals }
    }

    /// Constrói a partir de `Decimal` em unidade *maior* (ex: "1.50 USD" não
    /// "150 cents"). Erro se valor tem mais precisão do que `decimals`
    /// suporta sem truncamento.
    pub fn from_decimal_exact(value: Decimal, decimals: u8) -> Result<Self, NumericError> {
        let scale = decimals as u32;
        let scaled = value
            .checked_mul(Decimal::from(10i64.pow(scale)))
            .ok_or_else(|| NumericError::Overflow {
                value: value.to_string(),
                scale,
            })?;
        if scaled != scaled.floor() {
            return Err(NumericError::NotAligned {
                value: value.to_string(),
                tick_size: format!("1e-{decimals}"),
            });
        }
        let minor = scaled.to_i128().ok_or_else(|| NumericError::Overflow {
            value: value.to_string(),
            scale,
        })?;
        Ok(Self { minor, decimals })
    }

    /// Reconstrói `Decimal` em unidade maior (sem perda).
    pub fn to_decimal(self) -> Decimal {
        let scale = self.decimals as u32;
        let sign = if self.minor < 0 { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
        let abs = self.minor.unsigned_abs();
        let dec = if abs <= i64::MAX as u128 {
            Decimal::from(abs as i64)
        } else {
            Decimal::from_str_exact(&abs.to_string()).expect("u128 → Decimal")
        };
        sign * dec / Decimal::from(10i64.pow(scale))
    }

    /// Soma. Decimais devem coincidir (mesma moeda).
    pub fn checked_add(self, other: Self) -> Result<Self, NumericError> {
        if self.decimals != other.decimals {
            return Err(NumericError::InvalidSize(format!(
                "currency precision mismatch: decimals {} vs {}",
                self.decimals, other.decimals
            )));
        }
        let minor = self.minor.checked_add(other.minor).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { minor, decimals: self.decimals })
    }

    /// Subtração.
    pub fn checked_sub(self, other: Self) -> Result<Self, NumericError> {
        if self.decimals != other.decimals {
            return Err(NumericError::InvalidSize(format!(
                "currency precision mismatch: decimals {} vs {}",
                self.decimals, other.decimals
            )));
        }
        let minor = self.minor.checked_sub(other.minor).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { minor, decimals: self.decimals })
    }

    /// Multiplicação por escalar inteiro.
    pub fn checked_mul_int(self, k: i128) -> Result<Self, NumericError> {
        let minor = self.minor.checked_mul(k).ok_or(NumericError::ArithmeticOverflow)?;
        Ok(Self { minor, decimals: self.decimals })
    }

    /// Sinal.
    pub fn is_negative(self) -> bool {
        self.minor < 0
    }

    /// Zero.
    pub fn zero(decimals: u8) -> Self {
        Self { minor: 0, decimals }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    #[test]
    fn usd_roundtrip() {
        let m = MoneyMinor::from_decimal_exact(dec!(1234.56), 2).unwrap();
        assert_eq!(m.minor, 123_456);
        assert_eq!(m.to_decimal(), dec!(1234.56));
    }

    #[test]
    fn btc_satoshis_roundtrip() {
        let m = MoneyMinor::from_decimal_exact(dec!(0.00000123), 8).unwrap();
        assert_eq!(m.minor, 123);
        assert_eq!(m.to_decimal(), dec!(0.00000123));
    }

    #[test]
    fn over_precision_rejects() {
        // 1234.567 com decimals=2 não é representável exato.
        let r = MoneyMinor::from_decimal_exact(dec!(1234.567), 2);
        assert!(matches!(r, Err(NumericError::NotAligned { .. })));
    }

    #[test]
    fn add_currency_mismatch_errors() {
        let usd = MoneyMinor::from_minor(100, 2);
        let btc = MoneyMinor::from_minor(100, 8);
        assert!(usd.checked_add(btc).is_err());
    }

    proptest! {
        #[test]
        fn add_associative(
            a in -1_000_000_000i128..1_000_000_000i128,
            b in -1_000_000_000i128..1_000_000_000i128,
            c in -1_000_000_000i128..1_000_000_000i128,
        ) {
            let ma = MoneyMinor::from_minor(a, 6);
            let mb = MoneyMinor::from_minor(b, 6);
            let mc = MoneyMinor::from_minor(c, 6);
            let left = ma.checked_add(mb).unwrap().checked_add(mc).unwrap();
            let right = ma.checked_add(mb.checked_add(mc).unwrap()).unwrap();
            prop_assert_eq!(left, right);
        }

        #[test]
        fn add_commutative(a in -1_000_000i128..1_000_000i128, b in -1_000_000i128..1_000_000i128) {
            let ma = MoneyMinor::from_minor(a, 6);
            let mb = MoneyMinor::from_minor(b, 6);
            prop_assert_eq!(ma.checked_add(mb).unwrap(), mb.checked_add(ma).unwrap());
        }

        #[test]
        fn decimal_roundtrip(minor in -1_000_000_000_000i128..1_000_000_000_000i128, dec in 0u8..10) {
            let m = MoneyMinor::from_minor(minor, dec);
            let d = m.to_decimal();
            let back = MoneyMinor::from_decimal_exact(d, dec).unwrap();
            prop_assert_eq!(back, m);
        }
    }
}
