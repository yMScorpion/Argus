use serde::{Deserialize, Serialize};

/// Identidade de processo dentro da suíte ARGUS.
///
/// Anexado a cada mensagem IPC para permitir validação de origem e
/// roteamento. Quem pode enviar o quê é descrito em
/// `docs/architecture/process-boundaries.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    /// ARGUS Terminal — render + chrome + emite intents.
    Terminal,
    /// ARGUS Data Plane — ingest + normalize + storage.
    DataPlane,
    /// ARGUS Risk/Execution Daemon — único com credenciais.
    RiskDaemon,
    /// ARGUS Research/ML/Marketplace.
    Research,
    /// Plugin sandboxed (WASM); sub-identity de Research ou Terminal.
    Plugin,
    /// CLI tool (argus-audit verify, argus-replay verify, etc).
    Cli,
    /// Test harness (não deve aparecer em produção).
    TestHarness,
}

impl ProcessRole {
    /// Indica se este role pode acessar credenciais de venue.
    pub fn can_access_venue_credentials(self) -> bool {
        matches!(self, Self::RiskDaemon)
    }

    /// Indica se este role pode emitir intents de execução.
    pub fn can_emit_execution_intents(self) -> bool {
        matches!(self, Self::Terminal | Self::Research | Self::Plugin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_risk_daemon_has_credentials() {
        for r in [
            ProcessRole::Terminal,
            ProcessRole::DataPlane,
            ProcessRole::Research,
            ProcessRole::Plugin,
            ProcessRole::Cli,
        ] {
            assert!(!r.can_access_venue_credentials(), "{r:?}");
        }
        assert!(ProcessRole::RiskDaemon.can_access_venue_credentials());
    }

    #[test]
    fn data_plane_does_not_emit_intents() {
        assert!(!ProcessRole::DataPlane.can_emit_execution_intents());
        assert!(!ProcessRole::RiskDaemon.can_emit_execution_intents());
    }
}
