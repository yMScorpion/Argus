//! Streams derivados: candles, CVD, footprint, indicadores incrementais.
//!
//! Fase 3 inicial: builders e tipos. Fase 9 expande para indicadores
//! completos.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod candle;
mod cvd;

pub use candle::{Candle, CandleBuilder, Timeframe};
pub use cvd::CvdAccumulator;
