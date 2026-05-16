//! Tipos fundamentais compartilhados pela suíte ARGUS.
//!
//! Este crate é a base do grafo de dependências; ele não pode depender de
//! crates de domínio. Apenas tipos canônicos:
//! - IDs opaque para evitar confusão entre domínios.
//! - Enumerações de venue / contract type / side.
//! - Referência a instrumento (estável entre processos).
//! - Identidade de processo (Terminal / Data Plane / Risk / Research).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ids;
mod instrument;
mod process;
mod side;
mod venue;

pub use ids::{
    ChannelId, ClientOrderId, CorrelationId, EventId, InstanceId, IntentId, InstrumentId,
    OrderId, PluginId, ReplaySessionId, SequenceId, SnapshotId, StrategyId, SubscriptionId,
    SymbolId, TraceId, TradeId,
};
pub use instrument::{ContractType, InstrumentRef, OptionKind, Settlement};
pub use process::ProcessRole;
pub use side::Side;
pub use venue::{Venue, VenueStatus};

/// Versão major do conjunto de tipos canônicos.
///
/// Bump major aqui obriga ADR + bridge para storage histórico.
pub const CORE_TYPES_VERSION: u16 = 0;
