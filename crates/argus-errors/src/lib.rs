//! Taxonomia de erros da suíte ARGUS.
//!
//! Estes erros são compartilhados entre processos via IPC e logs. Cada
//! variante carrega:
//!
//! - **code**: identificador estável (string slug) para correlação com docs
//!   e runbooks.
//! - **severity**: nível de gravidade que determina rota de notificação.
//! - **retryability**: se retry automático é seguro.
//! - **user_message**: mensagem amigável para mostrar no Terminal.
//! - **internal_context**: detalhes ricos para logs estruturados.
//!
//! # Por que não usar `anyhow::Error` em fronteira de processo
//!
//! `anyhow::Error` não tem code estável; mensagens variam entre versões;
//! tipos não preservam ao cruzar serialização. `ArgusError` é o tipo
//! canônico que cruza fronteiras; `anyhow::Error` segue útil em caminhos
//! internos de um único crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// Severidade do erro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informacional — não bloqueia operação.
    Info,
    /// Aviso — degrada confidence mas serviço útil.
    Warning,
    /// Erro recuperável (com retry, fallback, ou intervenção mínima).
    Error,
    /// Erro crítico — exige humano; pode pausar componente.
    Critical,
    /// Fatal — componente entra em estado seguro (panic safe / read only).
    Fatal,
}

/// Indicador de retry-safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retryable {
    /// Retry automático seguro (operação idempotente, erro transiente).
    Auto,
    /// Retry exige confirmação humana (ambiguidade no estado externo).
    Manual,
    /// Não retentar — refazer a operação produziria efeito adverso.
    Never,
}

/// Taxonomia canônica de erros ARGUS.
#[derive(Debug, thiserror::Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArgusError {
    /// Erro de configuração (config malformada, valor inválido).
    #[error("config error: {message}")]
    Config {
        /// Detalhe.
        message: String,
    },

    /// Erro de schema (campo faltando, versão incompatível, parse).
    #[error("schema error: {message} (schema_version={schema_version:?})")]
    Schema {
        /// Detalhe.
        message: String,
        /// Versão do schema (se conhecida).
        schema_version: Option<u16>,
    },

    /// Erro de IPC (transporte, framing, deserialização).
    #[error("ipc error: {message}")]
    Ipc {
        /// Detalhe.
        message: String,
    },

    /// Erro de shared memory.
    #[error("shm error: {message}")]
    Shm {
        /// Detalhe.
        message: String,
    },

    /// Gap detectado em data stream.
    #[error("data gap: venue={venue} symbol={symbol} channel={channel} expected={expected_seq} got={got_seq}")]
    DataGap {
        /// Venue.
        venue: String,
        /// Símbolo.
        symbol: String,
        /// Canal.
        channel: String,
        /// Sequence esperada.
        expected_seq: u64,
        /// Sequence recebida.
        got_seq: u64,
    },

    /// Erro de venue (HTTP, WS, payload).
    #[error("venue error: venue={venue} {message}")]
    Venue {
        /// Venue.
        venue: String,
        /// Detalhe.
        message: String,
    },

    /// Rate limit atingido.
    #[error("rate limit: venue={venue} retry_after_ms={retry_after_ms}")]
    RateLimit {
        /// Venue.
        venue: String,
        /// Milissegundos sugeridos antes de retry.
        retry_after_ms: u64,
    },

    /// Erro de precisão / arredondamento.
    #[error("precision error: {message}")]
    Precision {
        /// Detalhe.
        message: String,
    },

    /// Intent rejected pelo risk envelope.
    #[error("risk reject: {reason}")]
    RiskReject {
        /// Razão concreta.
        reason: String,
    },

    /// Timeout de execução.
    #[error("execution timeout: intent_id={intent_id} after_ms={elapsed_ms}")]
    ExecutionTimeout {
        /// Intent ID.
        intent_id: String,
        /// Tempo decorrido em ms.
        elapsed_ms: u64,
    },

    /// Reconciliation diff entre estado local e venue.
    #[error("reconciliation error: {message}")]
    Reconciliation {
        /// Detalhe.
        message: String,
    },

    /// Erro de storage (disco, parquet, duckdb).
    #[error("storage error: {message}")]
    Storage {
        /// Detalhe.
        message: String,
    },

    /// Divergência em replay determinístico.
    #[error("replay divergence: at_ts={at_ts_ns} expected_hash={expected_hash} got_hash={got_hash}")]
    ReplayDivergence {
        /// Timestamp da divergência.
        at_ts_ns: u64,
        /// Hash esperado.
        expected_hash: String,
        /// Hash obtido.
        got_hash: String,
    },

    /// Plugin tentou exercer capability não autorizada.
    #[error("plugin capability error: plugin={plugin_id} requested={capability}")]
    PluginCapability {
        /// Plugin ID.
        plugin_id: String,
        /// Capability requested.
        capability: String,
    },

    /// Erro de segurança (sandbox breach, credential, signing).
    #[error("security error: {message}")]
    Security {
        /// Detalhe (NUNCA logar secret aqui).
        message: String,
    },

    /// Erro interno inesperado (panic recuperado, invariant violation).
    #[error("internal: {message}")]
    Internal {
        /// Detalhe.
        message: String,
    },
}

impl ArgusError {
    /// Code slug estável.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Config { .. } => "argus.config",
            Self::Schema { .. } => "argus.schema",
            Self::Ipc { .. } => "argus.ipc",
            Self::Shm { .. } => "argus.shm",
            Self::DataGap { .. } => "argus.data_gap",
            Self::Venue { .. } => "argus.venue",
            Self::RateLimit { .. } => "argus.rate_limit",
            Self::Precision { .. } => "argus.precision",
            Self::RiskReject { .. } => "argus.risk_reject",
            Self::ExecutionTimeout { .. } => "argus.execution_timeout",
            Self::Reconciliation { .. } => "argus.reconciliation",
            Self::Storage { .. } => "argus.storage",
            Self::ReplayDivergence { .. } => "argus.replay_divergence",
            Self::PluginCapability { .. } => "argus.plugin_capability",
            Self::Security { .. } => "argus.security",
            Self::Internal { .. } => "argus.internal",
        }
    }

    /// Severidade canônica.
    pub fn severity(&self) -> Severity {
        match self {
            Self::Config { .. } => Severity::Error,
            Self::Schema { .. } => Severity::Critical,
            Self::Ipc { .. } => Severity::Error,
            Self::Shm { .. } => Severity::Critical,
            Self::DataGap { .. } => Severity::Warning,
            Self::Venue { .. } => Severity::Warning,
            Self::RateLimit { .. } => Severity::Info,
            Self::Precision { .. } => Severity::Error,
            Self::RiskReject { .. } => Severity::Info,
            Self::ExecutionTimeout { .. } => Severity::Error,
            Self::Reconciliation { .. } => Severity::Critical,
            Self::Storage { .. } => Severity::Error,
            Self::ReplayDivergence { .. } => Severity::Critical,
            Self::PluginCapability { .. } => Severity::Warning,
            Self::Security { .. } => Severity::Fatal,
            Self::Internal { .. } => Severity::Critical,
        }
    }

    /// Política de retry padrão.
    pub fn retryable(&self) -> Retryable {
        match self {
            Self::RateLimit { .. } | Self::Ipc { .. } | Self::Venue { .. } => Retryable::Auto,
            Self::DataGap { .. } | Self::ExecutionTimeout { .. } => Retryable::Manual,
            Self::Config { .. }
            | Self::Schema { .. }
            | Self::Shm { .. }
            | Self::Precision { .. }
            | Self::RiskReject { .. }
            | Self::Reconciliation { .. }
            | Self::Storage { .. }
            | Self::ReplayDivergence { .. }
            | Self::PluginCapability { .. }
            | Self::Security { .. }
            | Self::Internal { .. } => Retryable::Never,
        }
    }

    /// Mensagem amigável para mostrar ao trader.
    pub fn user_message(&self) -> String {
        match self {
            Self::DataGap { venue, .. } => {
                format!("Dados de {venue} apresentaram gap; resync em andamento.")
            }
            Self::RateLimit { venue, .. } => {
                format!("Rate limit atingido em {venue}; aguardando.")
            }
            Self::RiskReject { reason } => format!("Intent rejeitada: {reason}"),
            Self::ExecutionTimeout { .. } => {
                "Timeout na execução; verificando estado da ordem.".into()
            }
            Self::Reconciliation { .. } => {
                "Inconsistência detectada com a venue; reconciliação manual necessária.".into()
            }
            Self::Security { .. } => "Erro de segurança crítico — contate suporte.".into(),
            _ => format!("{self}"),
        }
    }
}

/// Result alias.
pub type Result<T> = std::result::Result<T, ArgusError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_slugs() {
        let e = ArgusError::DataGap {
            venue: "binance".into(),
            symbol: "BTCUSDT".into(),
            channel: "depth".into(),
            expected_seq: 100,
            got_seq: 105,
        };
        assert_eq!(e.code(), "argus.data_gap");
        assert_eq!(e.severity(), Severity::Warning);
        assert_eq!(e.retryable(), Retryable::Manual);
    }

    #[test]
    fn security_is_fatal() {
        let e = ArgusError::Security { message: "redacted".into() };
        assert_eq!(e.severity(), Severity::Fatal);
        assert_eq!(e.retryable(), Retryable::Never);
    }

    #[test]
    fn serde_roundtrip() {
        let e = ArgusError::RiskReject { reason: "size > envelope".into() };
        let j = serde_json::to_string(&e).unwrap();
        let back: ArgusError = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
    }
}
