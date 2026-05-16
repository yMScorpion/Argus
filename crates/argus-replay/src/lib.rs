//! Replay determinístico de eventos canônicos a partir do storage.
//!
//! Fluxo:
//!
//! 1. Carrega o **checkpoint** mais próximo de `target_ts` (≤).
//! 2. Re-aplica os eventos persistidos desde aquele checkpoint até `target_ts`.
//! 3. A partir daí, alimenta eventos em ordem para um consumidor (chart,
//!    backtest, strategy).
//!
//! # Branching
//!
//! Uma sessão de replay pode produzir uma **branch** que armazena decisões
//! ("e se eu tivesse entrado aqui?") e paper orders sem duplicar market data.
//! Branches são leves; apenas overlay sobre a sessão base.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod session;

pub use session::{
    BranchOverlay, ReplayDecision, ReplayError, ReplayPaperOrder, ReplaySession,
};
