use serde::{Deserialize, Serialize};

use argus_decimal::MoneyMinor;

/// Envelope de risco aplicado a toda intent antes de virar ordem.
///
/// Fase 1: estrutura + validação básica. Fase 10 expande para per-venue,
/// per-symbol, time-of-day, cooldown engine, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskEnvelope {
    /// Versão do envelope (incrementa em cada mudança).
    pub version: u32,
    /// Daily loss cap em sub-unidade da moeda quote (ex: cents USD).
    pub daily_loss_cap: MoneyMinor,
    /// Per-trade max risk em sub-unidade.
    pub per_trade_max_risk: MoneyMinor,
    /// Max notional por posição em sub-unidade.
    pub per_position_max_notional: MoneyMinor,
    /// Exposição máxima open total em sub-unidade.
    pub total_open_exposure_max: MoneyMinor,
    /// Trades por dia máximo.
    pub trades_per_day_max: u32,
    /// Cooldown após N perdas seguidas.
    pub cooldown_after_losses: CooldownPolicy,
    /// Max leverage padrão (multiplicado por 100 para reter precisão).
    pub default_max_leverage_x100: u32,
    /// Posições concorrentes max.
    pub max_concurrent_positions: u32,
}

/// Política de cooldown após perdas seguidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CooldownPolicy {
    /// Número de perdas seguidas para disparar.
    pub consecutive_losses: u32,
    /// Cooldown em minutos.
    pub cooldown_minutes: u32,
}

impl RiskEnvelope {
    /// Constrói envelope conservador padrão (para testes).
    pub fn conservative_default() -> Self {
        Self {
            version: 1,
            daily_loss_cap: MoneyMinor::from_decimal_exact(rust_decimal::Decimal::from(500), 2)
                .expect("static value"),
            per_trade_max_risk: MoneyMinor::from_decimal_exact(rust_decimal::Decimal::from(50), 2)
                .expect("static value"),
            per_position_max_notional: MoneyMinor::from_decimal_exact(
                rust_decimal::Decimal::from(5_000),
                2,
            )
            .expect("static value"),
            total_open_exposure_max: MoneyMinor::from_decimal_exact(
                rust_decimal::Decimal::from(15_000),
                2,
            )
            .expect("static value"),
            trades_per_day_max: 8,
            cooldown_after_losses: CooldownPolicy {
                consecutive_losses: 3,
                cooldown_minutes: 30,
            },
            default_max_leverage_x100: 25_00, // 25x
            max_concurrent_positions: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conservative_default_is_sane() {
        let e = RiskEnvelope::conservative_default();
        assert_eq!(e.version, 1);
        assert!(e.daily_loss_cap.minor > 0);
        assert!(e.per_trade_max_risk.minor < e.daily_loss_cap.minor);
    }
}

