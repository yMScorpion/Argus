use serde::{Deserialize, Serialize};

/// Política de arredondamento para conversões `Decimal -> Ticks/Lots`.
///
/// Toda conversão exige escolha explícita. Não há default — esquecer a
/// escolha é bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    /// Arredonda para o tick mais próximo abaixo (truncate em direção a -∞).
    Down,
    /// Arredonda para o tick mais próximo acima (truncate em direção a +∞).
    Up,
    /// Arredonda para o tick mais próximo; metade arredonda **para zero**
    /// (banker rule conservadora — fees nunca infladas em half-cases).
    NearestTowardZero,
    /// Falha se não estiver exatamente alinhado a um tick.
    RejectIfNotAligned,
}

/// Erros de conversão e aritmética numérica.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum NumericError {
    /// Valor não está alinhado a tick e política é `RejectIfNotAligned`.
    #[error("value {value} not aligned to tick size {tick_size}")]
    NotAligned {
        /// Valor passado.
        value: String,
        /// Tick size esperado.
        tick_size: String,
    },
    /// Overflow ao converter Decimal para inteiro escalado.
    #[error("overflow converting {value} (scale {scale}) to scaled integer")]
    Overflow {
        /// Valor que causou overflow.
        value: String,
        /// Scale tentado.
        scale: u32,
    },
    /// Tick size inválido (zero, negativo, ou com expoente que não cabe).
    #[error("invalid tick/lot size: {0}")]
    InvalidSize(String),
    /// Divisão por zero.
    #[error("division by zero")]
    DivByZero,
    /// Resultado aritmético excede i128.
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
}
