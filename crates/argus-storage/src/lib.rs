//! ARGUS Storage — persistência de eventos canônicos + checkpoints + segment
//! index, base para Replay determinístico (Fase 5).
//!
//! # Tiers
//!
//! A spec define três tiers:
//!
//! - **Hot tier** — últimos 30 minutos de eventos em RAM (ring buffer +
//!   arena). Implementado em [`hot::HotStore`].
//! - **Warm tier** — eventos compactados em disco; spec mandata
//!   DuckDB + Parquet. Esta versão implementa [`jsonl::JsonlEventStore`]
//!   como camada intermediária; migração para Parquet está documentada em
//!   `docs/architecture/adr/ADR-0008-storage-implementation-plan.md`.
//! - **Cold tier** — Parquet em disco lento ou S3. Stub neste momento.
//!
//! # Checkpoints
//!
//! [`checkpoint::Checkpoint`] serializa o estado consolidado (orderbook +
//! sequence + schema version + code version) periodicamente. Replay carrega
//! o checkpoint mais próximo de `target_ts`, depois re-aplica os eventos
//! subsequentes até `target_ts`.
//!
//! # Segment index
//!
//! [`index::SegmentIndex`] mapeia `(symbol, timestamp)` → `(segment_id,
//! offset)`. Permite seek O(log n) em horizontes históricos longos.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod checkpoint;
pub mod hot;
pub mod index;
pub mod jsonl;

pub use checkpoint::{Checkpoint, CheckpointError, CheckpointStore};
pub use hot::{HotStore, HotStoreStats};
pub use index::{SegmentIndex, SegmentIndexEntry};
pub use jsonl::{JsonlEventStore, JsonlReader, JsonlStoreError};

use argus_schema::CanonicalEvent;

/// Trait abstrata para qualquer backend de event store durável.
///
/// Implementações esperadas:
/// - [`JsonlEventStore`] — Fase 5 inicial.
/// - `ParquetEventStore` — Fase 5.1 (ADR-0008).
/// - `ClickHouseEventStore` — opcional (power user).
pub trait EventStore {
    /// Erro específico do backend.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Persiste um evento. Pode bufferizar — flush é chamada explicitamente.
    fn append(&mut self, event: &CanonicalEvent) -> Result<(), Self::Error>;

    /// Força flush dos buffers para disco.
    fn flush(&mut self) -> Result<(), Self::Error>;

    /// Total de eventos persistidos.
    fn count(&self) -> u64;
}
