//! Risk Daemon state machine + envelope (Fase 10).
//!
//! Esta versão da Fase 1-4 só fornece os tipos e a state machine
//! determinística. Validação de intents, audit hooks e venue interaction
//! entram nas fases 10-13.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod envelope;
mod state;

pub use envelope::RiskEnvelope;
pub use state::{DaemonState, StateTransition, TransitionError};
