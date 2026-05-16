# ADR-0009 — Fundações de observabilidade

## Status
Accepted (2026-05-15)

## Contexto

Spec §14 mandata métricas Prometheus, tracing OpenTelemetry, logs
estruturados, health checks e dashboard interno de diagnóstico. Observable
**first-class** — não bolt-on depois.

Mas a infra completa (push gateway, OTLP collector, dashboards externos) é
operacional, não de produto, e adicioná-la na Fase 6 antes de termos
qualquer connector real seria over-engineering.

## Decisão

Fase 6 entrega as **fundações in-process**:

1. **`MetricsRegistry`** thread-safe com famílias de Counter/Gauge/
   Histogram + labels canônicos. API simples e estável.
2. **`PrometheusExporter`** text exposition format. Renderiza qualquer
   registry em texto compatível com `prometheus.io`.
3. **`Sensitive<T>`** wrapper que redacta valor em `Debug`/`Display` —
   barreira tipada contra vazamento de secrets em logs (spec §13.4).
4. **`init_json_logging`** tracing-subscriber em JSON. Filter via
   `RUST_LOG`. Idempotente.
5. **`Trace` / `TraceStage`** primitivos para correlation cross-process
   (mas sem exporter remoto ainda).

Não entregamos nesta fase:

- HTTP serving de `/metrics` (Fase 6.1 quando connector real precisar dele).
- OTLP exporter remoto.
- Sample ring de últimos 100ms de spans (debug feature; Fase 7+).
- Compile-time check para Sensitive em logs (Clippy custom rule;
  considerado pós-MVP).

## Alternativas

- Adotar `prometheus` crate direto: rejeitado para evitar dep pesada em
  fase fundamental e para manter ownership do format/labels canônicos.
- Adotar `opentelemetry-otlp` direto: precisa runtime async, fora do
  escopo Fase 6.
- Pular Fase 6 e adicionar tudo na 7: rejeitado, observability tem que
  começar cedo para validar latency budgets em fixture.

## Consequências

**Boas:**
- Toda métrica nova tem casa óbvia (registry).
- Spec compliance (§14) já parcialmente atendida.
- Sensitive<T> previne classe inteira de bugs (logs com keys).

**Ruins:**
- Exporter HTTP fica para depois — operacional ainda manual via dumps.
- `prometheus` crate eventualmente vai ser pulled mesmo assim; quando
  isso acontecer, podemos remover `PrometheusExporter` próprio (ou
  manter como adapter para registry).

## Invariantes

- Tipos sensitive nunca emitem valor literal em representações textuais.
- Métricas com mesma `(name, labels)` retornam **a mesma instância** do
  registry — idempotência crítica para componentes que precisam alcançar
  o mesmo counter sem coordenação global.
- Prometheus output: labels sempre sorted (BTreeMap-style) para diff
  estável.

## Testes obrigatórios

- `argus_observability::sensitive::tests::debug_does_not_emit_value`.
- `argus_observability::registry::tests::counter_registration_is_idempotent`.
- `argus_observability::prometheus::tests::counter_is_rendered`.
- `argus_observability::prometheus::tests::label_values_are_escaped`.

## Próximas iterações

- **6.1**: `axum`-based `/metrics` server (local-bind). OTLP exporter.
- **6.2**: Histogram nativo Prometheus (cumulative buckets).
- **7+**: Diagnostic panel UI consumindo o registry via IPC.
