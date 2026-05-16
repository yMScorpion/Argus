//! Tipos numéricos escalados para o hot path de execução e risco.
//!
//! # Política
//!
//! Conforme ADR-0002, **caminho de execução, risco e contabilidade usa
//! apenas inteiros escalados**:
//!
//! - [`PriceTicks`] — preço como número de ticks (i128).
//! - [`QtyLots`] — quantidade como número de lots (i128).
//! - [`MoneyMinor`] — valores monetários em sub-unidade (centavos / sats).
//! - [`BasisPoints`] — fees, funding rates, slippage (1 bp = 1/10000).
//!
//! `f64` é proibido em qualquer crate que toque execução ou risco. Conversões
//! `Decimal -> Ticks/Lots` exigem [`Rounding`] explícito.
//!
//! # Por que i128 e não Decimal
//!
//! `Decimal` é correto mas mais lento (BCD interno + alocação em algumas
//! ops). Para o caminho quente, comparações de preço entre níveis de
//! orderbook precisam ser `==` em tempo de instrução — `i128` cumpre.
//!
//! `Decimal` segue útil em interfaces (input/output humano) e camadas
//! analíticas (backtest, indicadores).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod basis_points;
mod errors;
mod i128_str;
mod money;
mod price;
mod qty;
mod size;

pub use basis_points::BasisPoints;
pub use errors::{NumericError, Rounding};
pub use money::MoneyMinor;
pub use price::PriceTicks;
pub use qty::QtyLots;
pub use size::{LotSize, TickSize};
