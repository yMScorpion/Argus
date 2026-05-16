//! ARGUS Data Plane — pipeline de ingestão, normalização e publicação.
//!
//! Esta versão (Fases 0-4) define os primitives:
//!
//! - [`Pipeline`] — orquestra connector → orderbook → quality → publish.
//! - [`PipelineStats`] — métricas top-level.
//! - [`ChannelQuality`] — tracker rolling de qualidade por canal.
//! - [`PublishedChannel`] — wrapper sobre `SpscRing<CanonicalEvent>` para
//!   produção do Data Plane e consumo do Terminal.
//!
//! Conectores reais (Binance, Bybit, OKX, Hyperliquid) entram na Fase 8;
//! storage (Parquet/DuckDB) entra na Fase 5.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod pipeline;
mod publisher;
mod quality;

pub use pipeline::{Pipeline, PipelineStats};
pub use publisher::PublishedChannel;
pub use quality::{ChannelQuality, ChannelQualitySnapshot};
