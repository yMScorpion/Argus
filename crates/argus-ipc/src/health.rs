//! Health protocol entre processos.
//!
//! Cada processo expõe o seu estado de saúde via `HealthSnapshot` que é
//! consumido por outros processos (notavelmente Terminal para banner global)
//! e ferramentas de operação. Conforme a spec, cada componente sinaliza um
//! dos estados abaixo, e transições significativas são auditadas.
//!
//! Fluxo de uso:
//!
//! ```
//! use argus_ipc::health::{HealthState, HealthSnapshot, SubsystemHealth};
//! use argus_core_types::{InstanceId, ProcessRole};
//! use argus_time::MonotonicNs;
//!
//! let mut snap = HealthSnapshot::new(
//!     ProcessRole::DataPlane,
//!     InstanceId::from_u128(1),
//!     MonotonicNs::from_nanos(123),
//! );
//! snap.set_state(HealthState::Ready);
//! snap.subsystem("binance_spot.depth")
//!     .set(SubsystemHealth::Green { since_ns: MonotonicNs::from_nanos(123) });
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use argus_core_types::{InstanceId, ProcessRole};
use argus_time::MonotonicNs;

/// Estados de saúde top-level conforme spec.
///
/// Transições válidas estão documentadas em
/// `docs/architecture/process-boundaries.md` e
/// `docs/architecture/invariants/risk-daemon.md` (para o Risk Daemon
/// especificamente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// Processo subiu mas ainda não terminou o bootstrap (config + recovery).
    Starting,
    /// Processo pronto para atender requisições normais.
    Ready,
    /// Processo opera com degradação (algum subsistema yellow/red).
    Degraded,
    /// Processo só atende leituras (não aceita comandos que mudam estado).
    ReadOnly,
    /// Risk Daemon bloqueia novas execuções (kill switch, data degraded,
    /// daily loss cap atingido, …).
    ExecutionBlocked,
    /// Recovery/reconciliação obrigatória antes de aceitar comandos.
    ReconcileRequired,
    /// Estado de pânico: tentativa de minimizar dano com cancel-all e alerta.
    PanicSafe,
    /// Processo encerrando graciosamente.
    Stopping,
}

impl HealthState {
    /// Indica se este estado permite aceitar novos comandos de execução
    /// (no contexto do Risk Daemon).
    pub fn accepts_execution(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Indica se este estado é terminal (não-recoverable sem restart).
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Stopping)
    }
}

/// Saúde de um subsistema individual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "color", content = "since")]
pub enum SubsystemHealth {
    /// Subsistema operando normalmente desde `since_ns` (monotonic).
    Green {
        /// Quando entrou em green.
        since_ns: MonotonicNs,
    },
    /// Subsistema com degradação tolerável.
    Yellow {
        /// Quando entrou em yellow.
        since_ns: MonotonicNs,
    },
    /// Subsistema indisponível ou em erro.
    Red {
        /// Quando entrou em red.
        since_ns: MonotonicNs,
    },
}

impl SubsystemHealth {
    /// Cor canônica (para Prometheus label / UI banner).
    pub fn color(self) -> &'static str {
        match self {
            Self::Green { .. } => "green",
            Self::Yellow { .. } => "yellow",
            Self::Red { .. } => "red",
        }
    }
}

/// Snapshot completo de saúde de um processo.
///
/// Construído por cada componente e exposto via IPC para Terminal/CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthSnapshot {
    /// Quem está reportando.
    pub process: ProcessRole,
    /// Instância (por boot).
    pub instance: InstanceId,
    /// Monotonic timestamp do snapshot.
    pub captured_at: MonotonicNs,
    /// Estado top-level.
    pub state: HealthState,
    /// Subsistemas (label → saúde). `BTreeMap` para ordering estável em
    /// snapshots — facilita comparação em testes/auditoria.
    pub subsystems: BTreeMap<String, SubsystemHealth>,
    /// Mensagem livre (opcional) — útil para razão de Degraded/Panic.
    pub message: Option<String>,
}

impl HealthSnapshot {
    /// Constrói snapshot novo em `Starting` sem subsistemas.
    pub fn new(process: ProcessRole, instance: InstanceId, captured_at: MonotonicNs) -> Self {
        Self {
            process,
            instance,
            captured_at,
            state: HealthState::Starting,
            subsystems: BTreeMap::new(),
            message: None,
        }
    }

    /// Define estado top-level.
    pub fn set_state(&mut self, state: HealthState) -> &mut Self {
        self.state = state;
        self
    }

    /// Define mensagem (limpa se `None`).
    pub fn set_message(&mut self, msg: Option<String>) -> &mut Self {
        self.message = msg;
        self
    }

    /// Acessa/insere um subsistema.
    pub fn subsystem(&mut self, name: &str) -> SubsystemEntry<'_> {
        SubsystemEntry { map: &mut self.subsystems, name: name.to_string() }
    }

    /// Pior cor entre os subsistemas (green > yellow > red).
    pub fn worst_subsystem(&self) -> Option<SubsystemHealth> {
        self.subsystems
            .values()
            .copied()
            .reduce(|a, b| match (a, b) {
                (SubsystemHealth::Red { .. }, _) | (_, SubsystemHealth::Red { .. }) => {
                    if matches!(a, SubsystemHealth::Red { .. }) {
                        a
                    } else {
                        b
                    }
                }
                (SubsystemHealth::Yellow { .. }, _) | (_, SubsystemHealth::Yellow { .. }) => {
                    if matches!(a, SubsystemHealth::Yellow { .. }) {
                        a
                    } else {
                        b
                    }
                }
                _ => a,
            })
    }
}

/// Builder fluente para subsystem health.
#[derive(Debug)]
pub struct SubsystemEntry<'a> {
    map: &'a mut BTreeMap<String, SubsystemHealth>,
    name: String,
}

impl<'a> SubsystemEntry<'a> {
    /// Define saúde do subsistema.
    pub fn set(self, value: SubsystemHealth) {
        self.map.insert(self.name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> HealthSnapshot {
        HealthSnapshot::new(
            ProcessRole::DataPlane,
            InstanceId::from_u128(7),
            MonotonicNs::from_nanos(10),
        )
    }

    #[test]
    fn default_state_is_starting() {
        let s = snap();
        assert_eq!(s.state, HealthState::Starting);
        assert!(s.subsystems.is_empty());
    }

    #[test]
    fn worst_subsystem_picks_red() {
        let mut s = snap();
        s.subsystem("a").set(SubsystemHealth::Green { since_ns: MonotonicNs::from_nanos(1) });
        s.subsystem("b").set(SubsystemHealth::Yellow { since_ns: MonotonicNs::from_nanos(2) });
        s.subsystem("c").set(SubsystemHealth::Red { since_ns: MonotonicNs::from_nanos(3) });
        let worst = s.worst_subsystem().unwrap();
        assert!(matches!(worst, SubsystemHealth::Red { .. }));
    }

    #[test]
    fn only_ready_accepts_execution() {
        assert!(HealthState::Ready.accepts_execution());
        for s in [
            HealthState::Starting,
            HealthState::Degraded,
            HealthState::ReadOnly,
            HealthState::ExecutionBlocked,
            HealthState::ReconcileRequired,
            HealthState::PanicSafe,
            HealthState::Stopping,
        ] {
            assert!(!s.accepts_execution(), "{s:?} should not accept exec");
        }
    }

    #[test]
    fn serde_roundtrip() {
        let mut s = snap();
        s.set_state(HealthState::Degraded);
        s.subsystem("storage").set(SubsystemHealth::Yellow {
            since_ns: MonotonicNs::from_nanos(42),
        });
        let j = serde_json::to_string(&s).unwrap();
        let back: HealthSnapshot = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }
}
