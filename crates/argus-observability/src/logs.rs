//! Inicialização de logs estruturados em JSON.
//!
//! Em produção usamos tracing-subscriber com formato JSON, level filter via
//! `RUST_LOG`, e campos fixos `component`, `instance_id`, `process`. Em
//! testes/dev pode usar formato humano.
//!
//! Regra (spec §13.4): nenhum log emite secret. Tipos sensitive usam
//! [`crate::Sensitive<T>`] cujo `Debug`/`Display` redactam.

use std::sync::Once;

static INIT: Once = Once::new();

/// Inicializa logger JSON global (uma vez por processo).
///
/// Se chamado mais de uma vez, faz no-op. Para reconfigurar precisa restart
/// do processo (limitação do tracing-subscriber global).
pub fn init_json_logging() {
    INIT.call_once(|| {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_current_span(false)
            .with_span_list(true)
            .finish();
        // try_init: caso já haja um global instalado (raro em testes), apenas
        // ignora.
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

/// Versão para testes: humano + DEBUG. Idempotente.
#[cfg(test)]
pub fn init_test_logging() {
    INIT.call_once(|| {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug"));
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        init_json_logging();
        init_json_logging();
        // Se chegou aqui sem panic, está OK.
    }
}
