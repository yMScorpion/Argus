//! IPC command bus tipado entre processos da suíte.
//!
//! - [`Envelope`] encapsula qualquer payload e adiciona origem, trace_id e
//!   body_hash.
//! - [`command::IpcCommand`] define os comandos canônicos do bus.
//! - [`health`] define o protocolo de health snapshots.
//!
//! Transporte concreto (Unix domain socket, named pipe, gRPC remoto) é
//! introduzido nas Fases 7/14 com TLS mutual auth quando o link for
//! não-local. Esta versão (Fase 2) só define **schemas**, suficientes para
//! testes de contrato.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod command;
pub mod health;

use serde::{Deserialize, Serialize};

use argus_core_types::{CorrelationId, InstanceId, ProcessRole, TraceId};
use argus_time::MonotonicNs;

/// Envelope que cerca toda mensagem IPC entre processos.
///
/// Garante origem identificável, versionamento de schema, correlação
/// distribuída e integridade via blake3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    /// Versão do schema do payload.
    pub schema_version: u16,
    /// Processo que enviou.
    pub sender_process: ProcessRole,
    /// Instância concreta (por boot).
    pub sender_instance: InstanceId,
    /// Monotonic clock no send.
    pub sent_at: MonotonicNs,
    /// Trace ID para correlação OTLP.
    pub trace_id: TraceId,
    /// Correlation ID multi-step (intent → order → fill).
    pub correlation_id: Option<CorrelationId>,
    /// Hash blake3 do payload serializado (detecta corrupção/adulteração).
    pub body_hash: [u8; 32],
    /// Payload tipado.
    pub body: T,
}

impl<T> Envelope<T>
where
    T: Serialize,
{
    /// Constrói envelope, computando body_hash via serde_json + blake3.
    ///
    /// Em fase 7+ migramos para Cap'n Proto e hashing direto do buffer wire,
    /// que evita a serialização duplicada.
    pub fn new(
        schema_version: u16,
        sender_process: ProcessRole,
        sender_instance: InstanceId,
        sent_at: MonotonicNs,
        trace_id: TraceId,
        correlation_id: Option<CorrelationId>,
        body: T,
    ) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(&body)?;
        let hash = blake3::hash(&bytes);
        let mut body_hash = [0u8; 32];
        body_hash.copy_from_slice(hash.as_bytes());
        Ok(Self {
            schema_version,
            sender_process,
            sender_instance,
            sent_at,
            trace_id,
            correlation_id,
            body_hash,
            body,
        })
    }

    /// Verifica integridade re-computando hash.
    pub fn verify(&self) -> Result<bool, serde_json::Error> {
        let bytes = serde_json::to_vec(&self.body)?;
        let hash = blake3::hash(&bytes);
        Ok(hash.as_bytes() == &self.body_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_time::MonotonicNs;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Probe {
        msg: String,
        n: u32,
    }

    #[test]
    fn envelope_verifies_integrity() {
        let p = Probe { msg: "hello".into(), n: 42 };
        let e = Envelope::new(
            0,
            ProcessRole::Terminal,
            InstanceId::from_u128(1),
            MonotonicNs::from_nanos(100),
            TraceId::from_u128(2),
            None,
            p.clone(),
        )
        .unwrap();
        assert!(e.verify().unwrap());
    }

    #[test]
    fn corrupted_body_detected() {
        let p = Probe { msg: "hello".into(), n: 42 };
        let mut e = Envelope::new(
            0,
            ProcessRole::Terminal,
            InstanceId::from_u128(1),
            MonotonicNs::from_nanos(100),
            TraceId::from_u128(2),
            None,
            p,
        )
        .unwrap();
        // Mexer no body sem atualizar hash deve ser detectável.
        e.body.n = 99;
        assert!(!e.verify().unwrap());
    }
}
