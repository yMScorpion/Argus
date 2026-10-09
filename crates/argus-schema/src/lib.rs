//! Schemas canônicos: `CanonicalEvent`, `Intent`, `OrderState`, payloads.
//!
//! Conforme ADR-0006, formato atual usa `serde` derive; arquivos `.capnp` em
//! `schemas/capnp/` declaram intenção e serão fonte para geração futura.
//!
//! # Versionamento
//!
//! - Constante `SCHEMA_VERSION` deve ser bumpada para qualquer mudança.
//! - Campos novos são `Option<T>` ou têm `#[serde(default)]`.
//! - Campos removidos ficam tombstoned por duas minors antes de remover.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod canonical;
mod data_quality;
mod payload;

pub use canonical::CanonicalEvent;
pub use data_quality::{DataQuality, QualityFlag};
pub use payload::{
    BookDelta, BookLevel, BookSnapshot, EventPayload, FundingPayload, IndexPricePayload,
    LiquidationPayload, MarkPricePayload, OpenInterestPayload, TradePayload,
};

/// Versão major do conjunto de schemas. Bump exige ADR + migration tool.
pub const SCHEMA_VERSION: u16 = 0;
