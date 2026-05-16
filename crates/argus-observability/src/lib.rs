//! ARGUS Observability — métricas Prometheus + tracing + logs estruturados.
//!
//! Spec §14:
//!
//! - **Metrics** — Prometheus exposition em endpoint local (`/metrics`).
//!   Contadores, gauges, histogramas com labels canônicos (venue, symbol,
//!   channel).
//! - **Tracing** — propagação OpenTelemetry de `trace_id` no hot path.
//!   Sample rate adjustable; debug ring de últimos 100ms.
//! - **Logs** — JSON estruturado com schema fixo. Sem secrets (compile-time
//!   check via [`Sensitive<T>`]).
//! - **Health** — endpoint `/health` por componente (já definido em
//!   [`argus_ipc::health`]).
//!
//! Esta versão (Fase 6) implementa:
//!
//! - [`registry::MetricsRegistry`] — registry global de métricas com labels.
//! - [`prometheus::PrometheusExporter`] — formato Prometheus text exposition.
//! - [`logs::init_json_logging`] — inicializa tracing-subscriber em modo JSON.
//! - [`sensitive::Sensitive<T>`] — wrapper que nunca emite valor em `Debug`/`Display`.
//! - [`trace::TraceId`] — re-export do `argus_core_types::TraceId` com helpers.
//!
//! OTLP exporter remoto fica para Fase 6.1 (precisa runtime async); esta
//! versão entrega a base estática.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod logs;
pub mod prometheus;
pub mod registry;
pub mod sensitive;
pub mod trace;

pub use prometheus::PrometheusExporter;
pub use registry::{Label, LabelSet, MetricsRegistry, MetricKind};
pub use sensitive::Sensitive;
