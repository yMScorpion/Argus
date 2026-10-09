use serde::{Deserialize, Serialize};

/// Estados do Risk Daemon (conforme `docs/architecture/invariants/risk-daemon.md` I-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DaemonState {
    /// Subiu, ainda inicializando.
    Booting,
    /// Aguardando config válida.
    WaitingForConfig,
    /// Sem credenciais carregadas.
    NoCredentials,
    /// Reconciliação obrigatória antes de aceitar intents.
    ReconcileRequired,
    /// Operação normal.
    Ready,
    /// Algum subsystem de dados degradado.
    DataDegraded,
    /// Alguma venue degradada.
    VenueDegraded,
    /// Execução bloqueada (envelope, killswitch, etc).
    ExecutionBlocked,
    /// Killswitch ativo.
    KillSwitchActive,
    /// Estado seguro pós-incidente; exige humano.
    PanicSafe,
    /// Em shutdown ordenado.
    ShuttingDown,
}

impl DaemonState {
    /// Indica se aceita intents para abrir novas posições.
    pub fn accepts_new_intents(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Indica se aceita intents reduce-only (fechar/diminuir).
    pub fn accepts_reduce_only_intents(self) -> bool {
        matches!(self, Self::Ready | Self::DataDegraded | Self::VenueDegraded)
    }

    /// Indica se mantém ordens server-side já abertas.
    pub fn maintains_open_orders(self) -> bool {
        !matches!(self, Self::ShuttingDown)
    }
}

/// Transição requerida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateTransition {
    /// Estado anterior.
    pub from: DaemonState,
    /// Estado novo.
    pub to: DaemonState,
    /// Razão.
    pub reason: TransitionReason,
}

/// Razão de transição.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionReason {
    /// Boot completou.
    BootCompleted,
    /// Config carregada.
    ConfigLoaded,
    /// Credenciais carregadas.
    CredentialsLoaded,
    /// Reconciliação concluída com sucesso.
    ReconciliationOk,
    /// Data quality caiu abaixo do limite.
    DataQualityDropped,
    /// Data quality recuperou.
    DataQualityRecovered,
    /// Venue degradou.
    VenueDegraded,
    /// Venue recuperou.
    VenueRecovered,
    /// Killswitch ativado.
    KillSwitchFired,
    /// Killswitch desativado.
    KillSwitchCleared,
    /// Incidente irrecuperável.
    PanicSafeEntered,
    /// Comando de shutdown.
    ShutdownRequested,
}

/// Erro de transição inválida.
#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    /// Transição não permitida pela state machine.
    #[error("invalid transition from {from:?} to {to:?}")]
    Invalid {
        /// De.
        from: DaemonState,
        /// Para.
        to: DaemonState,
    },
}

impl DaemonState {
    /// Computa próximo estado dado razão, ou erro se transição inválida.
    pub fn next(self, reason: TransitionReason) -> Result<Self, TransitionError> {
        use DaemonState as S;
        use TransitionReason as R;

        let next = match (self, reason) {
            (S::Booting, R::BootCompleted) => S::WaitingForConfig,
            (S::WaitingForConfig, R::ConfigLoaded) => S::NoCredentials,
            (S::NoCredentials, R::CredentialsLoaded) => S::ReconcileRequired,
            (S::ReconcileRequired, R::ReconciliationOk) => S::Ready,
            (S::ReconcileRequired, R::PanicSafeEntered) => S::PanicSafe,

            (S::Ready, R::DataQualityDropped) => S::DataDegraded,
            (S::DataDegraded, R::DataQualityRecovered) => S::Ready,
            (S::Ready, R::VenueDegraded) => S::VenueDegraded,
            (S::VenueDegraded, R::VenueRecovered) => S::Ready,
            (S::Ready, R::KillSwitchFired)
            | (S::DataDegraded, R::KillSwitchFired)
            | (S::VenueDegraded, R::KillSwitchFired) => S::KillSwitchActive,
            (S::KillSwitchActive, R::KillSwitchCleared) => S::ExecutionBlocked,
            (S::ExecutionBlocked, R::ReconciliationOk) => S::Ready,
            (_, R::PanicSafeEntered) => S::PanicSafe,
            (S::PanicSafe, R::ReconciliationOk) => S::ReconcileRequired,
            (_, R::ShutdownRequested) => S::ShuttingDown,
            (from, _) => {
                return Err(TransitionError::Invalid { from, to: self });
            }
        };
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_to_ready_path() {
        let mut s = DaemonState::Booting;
        s = s.next(TransitionReason::BootCompleted).unwrap();
        assert_eq!(s, DaemonState::WaitingForConfig);
        s = s.next(TransitionReason::ConfigLoaded).unwrap();
        assert_eq!(s, DaemonState::NoCredentials);
        s = s.next(TransitionReason::CredentialsLoaded).unwrap();
        assert_eq!(s, DaemonState::ReconcileRequired);
        s = s.next(TransitionReason::ReconciliationOk).unwrap();
        assert_eq!(s, DaemonState::Ready);
    }

    #[test]
    fn ready_accepts_intents() {
        assert!(DaemonState::Ready.accepts_new_intents());
        assert!(!DaemonState::DataDegraded.accepts_new_intents());
        assert!(DaemonState::DataDegraded.accepts_reduce_only_intents());
        assert!(!DaemonState::PanicSafe.accepts_reduce_only_intents());
    }

    #[test]
    fn killswitch_path() {
        let mut s = DaemonState::Ready;
        s = s.next(TransitionReason::KillSwitchFired).unwrap();
        assert_eq!(s, DaemonState::KillSwitchActive);
        s = s.next(TransitionReason::KillSwitchCleared).unwrap();
        assert_eq!(s, DaemonState::ExecutionBlocked);
    }

    #[test]
    fn panic_safe_from_anywhere() {
        // Trying R::PanicSafeEntered from Ready (catch-all _).
        let s = DaemonState::Ready;
        let n = s.next(TransitionReason::PanicSafeEntered).unwrap();
        assert_eq!(n, DaemonState::PanicSafe);
    }

    #[test]
    fn invalid_transition_rejected() {
        // Ready não pode receber CredentialsLoaded.
        let s = DaemonState::Ready;
        assert!(s.next(TransitionReason::CredentialsLoaded).is_err());
    }
}
