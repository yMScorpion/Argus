//! Modelo de tempo da suíte ARGUS.
//!
//! # Princípios
//!
//! Cada evento canônico carrega **três** timestamps:
//!
//! - `exchange_ts_ns`: quando a venue diz que o evento ocorreu (sujeito a
//!   skew entre venues).
//! - `recv_ts_ns`: quando nosso socket recebeu o frame (relógio nosso, ground
//!   truth comum cross-venue).
//! - `process_ts_ns`: quando o evento saiu do normalizer (mede latência
//!   interna).
//!
//! Ordenação canônica de eventos:
//!
//! 1. `sequence_id` dentro do mesmo canal (autoritativo quando contínuo).
//! 2. `exchange_ts` cross-channel ou inter-venue (com tolerância).
//! 3. `recv_ts` apenas como último recurso (logado como degraded).
//!
//! # Relógios
//!
//! - Wall clock (Unix epoch) para timestamps externos.
//! - Monotonic clock para latency budgets internos (imune a NTP jumps).
//!
//! Se monotonic e wall divergem em > 5s entre eventos consecutivos, marca
//! janela como `ClockDegraded`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod clock;
mod stamps;
mod watermark;

pub use clock::{Clock, MonotonicClock, RealClock, TestClock};
pub use stamps::{
    duration_ns, ExchangeTs, MonotonicNs, ProcessTs, RecvTs, TimestampError, UnixNanos,
};
pub use watermark::Watermark;
