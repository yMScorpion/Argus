# Fase 6 — Observabilidade

## Objetivo

Instrumentar **antes de otimizar**. Toda métrica que aparecerá em produção
(latency, throughput, gap rates, frame times, intent acceptance rate) tem
casa pronta desde a Fase 6.

Sem observability, otimização vira intuição cega.

## Escopo entregue

- `argus-observability` crate com:
  - `MetricsRegistry` (Counter, Gauge, Histogram) com labels canônicos.
  - `PrometheusExporter` em text exposition format.
  - `Sensitive<T>` wrapper que redacta valor em `Debug`/`Display`.
  - `init_json_logging` (tracing-subscriber).
  - `Trace` / `TraceStage` primitivos para correlation cross-process.

## Não-escopo (Fase 6.1+)

- HTTP server bind para `/metrics`.
- OTLP exporter remoto.
- Histogram Prometheus nativo (cumulative buckets).
- Compile-time check para uso de `Sensitive` em logs.
- Diagnostic dashboard UI.

## Critérios de pronto

- ✅ Métricas com mesmo `(name, labels)` retornam mesma instância.
- ✅ `PrometheusExporter::render()` produz output parseável.
- ✅ Label values com `\`, `"`, `\n` são escapados corretamente.
- ✅ `Sensitive<T>` nunca emite valor em `Debug`/`Display`.
- ✅ `init_json_logging` é idempotente.
- ✅ Todos os testes passam (`cargo test -p argus-observability`).

## Como instrumentar um novo componente

```rust
use argus_observability::{Label, LabelSet, MetricsRegistry};

let registry = MetricsRegistry::new(); // ou compartilhar instância global

let events = registry.counter(
    "argus_dp_events_total",
    "Total canonical events processed",
    LabelSet::new(vec![
        Label::new("venue", "binance"),
        Label::new("symbol", "BTCUSDT"),
    ]),
);

events.inc(1);
```

Para histograms de latência:

```rust
let lat = registry.histogram(
    "argus_dp_decode_latency_ns",
    "WS frame decode latency",
    LabelSet::new(vec![Label::new("venue", "binance")]),
);

let t0 = std::time::Instant::now();
// ... decode work ...
lat.observe(t0.elapsed().as_nanos() as u64);
```

Para secrets:

```rust
use argus_observability::Sensitive;

struct ApiCredentials {
    key: String,
    secret: Sensitive<String>, // nunca aparece em logs
}
```

## Métricas canônicas (a serem instrumentadas em fases seguintes)

Conforme spec §14.1, nomenclatura:

**Data Plane:**
- `argus_dp_events_total{venue, symbol, type}`
- `argus_dp_decode_latency_ns_bucket`
- `argus_dp_normalize_latency_ns_bucket`
- `argus_dp_orderbook_apply_latency_ns_bucket`
- `argus_dp_gap_total{venue, symbol}`
- `argus_dp_reconnect_total{venue}`
- `argus_dp_confidence_score{venue, symbol, channel}`
- `argus_dp_shm_queue_depth`

**Risk Daemon (Fase 10+):**
- `argus_risk_intents_total{source, result, reason}`
- `argus_risk_validation_latency_ns_bucket`
- `argus_risk_orders_submitted_total`
- `argus_risk_ack_latency_ns_bucket`
- `argus_risk_reconciliation_diff_total`
- `argus_risk_killswitch_total`

**Terminal (Fase 7+):**
- `argus_terminal_frame_time_ns_bucket`
- `argus_terminal_frames_dropped_total`
- `argus_terminal_shm_lag_ns`

**Storage:**
- `argus_storage_write_latency_ns_bucket`
- `argus_storage_queue_depth`
- `argus_storage_disk_usage_bytes`
