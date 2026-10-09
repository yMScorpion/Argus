//! Métricas in-process leves.
//!
//! Para a fase atual (0-4), exportamos contadores, gauges e histogramas
//! HDR-like simples sem dependência Prometheus. Fase 6 adiciona exporter
//! Prometheus + integration OpenTelemetry.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod counter;
mod histogram;

pub use counter::{Counter, Gauge};
pub use histogram::{Histogram, HistogramSnapshot};
