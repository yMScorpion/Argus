//! Command bus tipado entre processos.
//!
//! A spec define um conjunto de comandos canônicos que cruzam processos:
//!
//! - `StartSubscription` / `StopSubscription` — Terminal → Data Plane.
//! - `GetHealth` — qualquer → qualquer.
//! - `GetCapabilities` — Terminal → Data Plane.
//! - `SubmitIntent` — Terminal/Research → Risk Daemon.
//! - `IntentResult` — Risk Daemon → Terminal/Research (callback).
//! - `RiskEnvelopeUpdate` — Terminal → Risk Daemon.
//! - `KillSwitch` — qualquer → Risk Daemon.
//! - `ReplayControl` — Terminal → Data Plane.
//! - `ConfigUpdate` — operador → qualquer.
//! - `Heartbeat` — Terminal ↔ Risk Daemon (watchdog).
//!
//! Cada comando vai dentro de um [`Envelope`](crate::Envelope) que adiciona
//! origem assinada, trace_id e body_hash. Esta camada é o **schema do
//! payload** — versionado conforme ADR-0006 (Cap'n Proto migration na Fase 7).
//!
//! # Status
//!
//! Esta versão (Fase 2) usa serde+JSON como wire format estável-temporário.
//! Na Fase 7 migramos para Cap'n Proto preservando os mesmos discriminants.

use serde::{Deserialize, Serialize};

use argus_core_types::{CorrelationId, InstanceId, ProcessRole, TraceId, Venue};

use crate::health::HealthSnapshot;

/// Comandos canônicos do command bus. Esta enum é **append-only**: novos
/// variants podem ser adicionados; remoções exigem migration window de duas
/// minor versions (ver ADR-0006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcCommand {
    /// Pede subscription a um stream específico.
    StartSubscription(SubscriptionRequest),
    /// Encerra subscription.
    StopSubscription(SubscriptionRef),
    /// Pede health snapshot.
    GetHealth,
    /// Resposta de GetHealth.
    HealthResponse(HealthSnapshot),
    /// Submete uma intent de execução (Terminal/Research → Risk Daemon).
    /// Body do intent é referenciado por id; o payload completo vive em outro
    /// canal (Fase 10).
    SubmitIntent(IntentRef),
    /// Resultado de processamento de intent.
    IntentResult(IntentResult),
    /// Atualiza envelope de risco; sempre acompanhado de cooldown obrigatório.
    RiskEnvelopeUpdate(RiskEnvelopeUpdate),
    /// Kill switch — cancela ordens abertas; opcionalmente flatten.
    KillSwitch(KillSwitchAction),
    /// Controla replay engine.
    ReplayControl(ReplayControl),
    /// Heartbeat para watchdog (Terminal → Risk Daemon).
    Heartbeat(HeartbeatPing),
}

/// Pedido de subscription a um canal de mercado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    /// Venue alvo.
    pub venue: Venue,
    /// Símbolo (`BTCUSDT`, `BTC-USD`, …) na grafia da venue.
    pub symbol: String,
    /// Canais pedidos (`trades`, `depth`, `funding`, …).
    pub channels: Vec<String>,
    /// ID de correlação para pareamento Start/Stop.
    pub correlation: CorrelationId,
}

/// Referência para subscription já estabelecida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionRef {
    /// Correlação atribuída no Start.
    pub correlation: CorrelationId,
}

/// Referência minimal para um intent (payload completo via outro canal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentRef {
    /// ID único da intent.
    pub intent_id: TraceId,
    /// Strategy ou source que emitiu.
    pub source: ProcessRole,
    /// Origem específica (strategy id, plugin id, …).
    pub source_instance: InstanceId,
}

/// Resultado de processamento de uma intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum IntentResult {
    /// Aceita; pode produzir múltiplas ordens.
    Accepted {
        /// Intent id correspondente.
        intent_id: TraceId,
    },
    /// Rejeitada por motivo do envelope ou validador.
    Rejected {
        /// Intent id correspondente.
        intent_id: TraceId,
        /// Razão canônica.
        reason: RejectReason,
    },
}

/// Razões canônicas de rejeição.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// Schema do intent inválido.
    SchemaInvalid,
    /// Daily loss cap atingido.
    DailyLossCap,
    /// Risk envelope excedido (qty, leverage, notional).
    RiskEnvelope,
    /// Símbolo fora da whitelist.
    SymbolNotWhitelisted,
    /// Cooldown ativo após N losses.
    Cooldown,
    /// Modo do daemon não aceita execução (DataDegraded, KillSwitch, …).
    StateBlocked,
    /// Venue indisponível.
    VenueUnavailable,
    /// Race condition / reconciliação pendente.
    ReconcileRequired,
    /// Outras razões com contexto livre.
    Other,
}

/// Update de envelope de risco; conforme spec exige cooldown obrigatório.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskEnvelopeUpdate {
    /// Versão pedida (deve ser monotonicamente crescente).
    pub version: u32,
    /// Hash blake3 do envelope serializado (payload em canal separado;
    /// hash protege contra IPC truncado / replay).
    pub envelope_hash: [u8; 32],
    /// Indica se a mudança aumenta limites (precisa cooldown).
    pub is_widening: bool,
}

/// Ação de kill switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillSwitchAction {
    /// Cancela todas ordens abertas.
    CancelAll,
    /// Cancela e fecha posições (market).
    CancelAndFlatten,
    /// Pausa novas intents por N minutos.
    BlockNew {
        /// Duração do bloqueio.
        minutes: u32,
    },
}

/// Controle de replay engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ReplayControl {
    /// Pause.
    Pause,
    /// Resume.
    Resume,
    /// Ajusta velocidade (1000 = 1.0x; 500 = 0.5x; 2000 = 2.0x).
    SetSpeed {
        /// Velocidade em milésimos.
        speed_x1000: u32,
    },
    /// Seek para timestamp absoluto.
    SeekUnix {
        /// Unix nanos alvo.
        target_ns: u64,
    },
    /// Cria branch a partir do estado atual.
    Branch {
        /// Identificador do branch.
        branch_id: u64,
    },
}

/// Heartbeat Terminal ↔ Risk Daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatPing {
    /// Monotonic do sender.
    pub monotonic_ns: u64,
    /// Sequência local; cumulative para detectar drops.
    pub seq: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::TraceId;

    #[test]
    fn command_roundtrip_kill_switch() {
        let cmd = IpcCommand::KillSwitch(KillSwitchAction::CancelAndFlatten);
        let j = serde_json::to_string(&cmd).unwrap();
        let back: IpcCommand = serde_json::from_str(&j).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn command_roundtrip_intent_result() {
        let cmd = IpcCommand::IntentResult(IntentResult::Rejected {
            intent_id: TraceId::from_u128(42),
            reason: RejectReason::DailyLossCap,
        });
        let j = serde_json::to_string(&cmd).unwrap();
        let back: IpcCommand = serde_json::from_str(&j).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn command_roundtrip_subscription() {
        let cmd = IpcCommand::StartSubscription(SubscriptionRequest {
            venue: Venue::BinanceSpot,
            symbol: "BTCUSDT".into(),
            channels: vec!["trades".into(), "depth".into()],
            correlation: CorrelationId::from_u128(100),
        });
        let j = serde_json::to_string(&cmd).unwrap();
        let back: IpcCommand = serde_json::from_str(&j).unwrap();
        assert_eq!(back, cmd);
    }

    #[test]
    fn heartbeat_is_small() {
        let h = HeartbeatPing { monotonic_ns: 12345, seq: 1 };
        let j = serde_json::to_string(&h).unwrap();
        // Sanidade: heartbeat deve ser pequeno (<128 bytes em JSON).
        assert!(j.len() < 128, "heartbeat JSON length = {}", j.len());
    }
}
