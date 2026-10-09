# Roadmap

| Fase | Nome                                | Resultado                               | Status |
| ---: | ----------------------------------- | --------------------------------------- | --- |
|    0 | Operating System do desenvolvimento | Monorepo, CI, ADRs, ownership           | ✅ |
|    1 | Tipos e schemas                     | Linguagem comum                         | ✅ |
|    2 | IPC/SHM foundation                  | Processos conversam com segurança       | ✅ |
|    3 | Data Plane sintético                | Pipeline sem exchange real              | ✅ |
|    4 | Orderbook engine                    | L2 correto e testado                    | ✅ |
|    5 | Storage/replay base                 | Histórico determinístico                | ✅ |
|    6 | Observabilidade                     | Métricas/traces/logs desde cedo         | ✅ |
|    7 | Terminal MVP                        | Um chart read-only                      | ⬜ |
|    8 | Connector real v1                   | Binance/read-only                       | ⬜ |
|    9 | Derived streams                     | CVD, candles, footprint básico          | ⬜ |
|   10 | Risk Daemon simulado                | Intents, envelope, audit                | ⬜ |
|   11 | Paper trading                       | Terminal → Risk → Sim Venue             | ⬜ |
|   12 | Testnet execution                   | Ordem real em testnet                   | ⬜ |
|   13 | Live guarded                        | Execução live limitada                  | ⬜ |
|   14 | Multi-venue data                    | Bybit/OKX/Hyperliquid/etc               | ⬜ |
|   15 | Heatmap/DOM                         | Orderflow visual avançado               | ⬜ |
|   16 | Replay UI                           | Time travel usável                      | ⬜ |
|   17 | Backtest v1                         | Research off-hot-path                   | ⬜ |
|   18 | Pattern candidates                  | SMC/VSA/Wyckoff/Elliott como candidates | ⬜ |
|   19 | Plugin runtime read-only            | WASM seguro para indicadores            | ⬜ |
|   20 | Strategy paper-only                 | Automation sem live                     | ⬜ |
|   21 | Smart routing                       | Multi-venue execution controlado        | ⬜ |
|   22 | Crypto-native feeds                 | Funding, OI, stablecoin, ETF, on-chain  | ⬜ |
|   23 | Security hardening                  | Secrets, signing, CSP, SBOM             | ⬜ |
|   24 | Marketplace fechado                 | Plugins internos verificados            | ⬜ |
|   25 | ML assistivo                        | Scores calibrados, sem auto-trade       | ⬜ |
|   26 | Mobile companion                    | Read-only + kill switch                 | ⬜ |
|   27 | Beta hardening                      | Chaos, recovery, release gates          | ⬜ |

## Notas de implementação por fase

### Fase 5 — Storage/Replay base ✅

Implementado em `crates/argus-storage` + `crates/argus-replay`:

- `HotStore` — ring em RAM por símbolo, evict por overflow.
- `JsonlEventStore` — backend warm-tier inicial (JSONL append-only). Rotação
  por tamanho de segmento. Migração para Parquet documentada em ADR-0008.
- `SegmentIndex` — sorted in-memory index `(instrument, ts) → (segment, offset)`
  com busca binária para seek.
- `Checkpoint` — snapshot determinístico do orderbook hash-verified via
  blake3. `CheckpointStore` persiste em arquivos imutáveis numerados.
- `ReplaySession` — `advance_to`, `seek`, `fork` (branching overlay sem
  duplicar market data), `verify_determinism` (hash bit-for-bit replay).

**Não implementado nesta fase** (Fase 5.1+):
- Backend Parquet/DuckDB (ADR-0008 marca path).
- Cold tier S3.
- Snapshots periódicos automáticos (precisa daemon worker).

### Fase 6 — Observabilidade ✅

Implementado em `crates/argus-observability`:

- `MetricsRegistry` — registry global thread-safe com famílias de Counter/
  Gauge/Histogram + labels canônicos (BTreeMap-style ordering estável).
- `PrometheusExporter` — text exposition format, escape correto de valores
  com `"`, `\`, `\n`.
- `Sensitive<T>` — wrapper que apaga conteúdo em `Debug`/`Display`. CI deve
  bloquear logs que vazem secrets via tipos não-`Sensitive`.
- `init_json_logging` — tracing-subscriber JSON com env filter via
  `RUST_LOG`. Idempotente.
- `TraceStage` / `Trace` — primitivos para propagação cross-process de
  `trace_id` (OTLP fica para Fase 6.1).

**Não implementado nesta fase** (Fase 6.1+):
- OTLP exporter remoto (precisa async runtime + retry).
- Ring de debug com último 100ms de spans.
- Health endpoint HTTP (já temos schema em `argus_ipc::health`).

## Próximo

Fase 7 — Terminal MVP (Tauri shell + uma janela wgpu). Branching/scrub da
Fase 16 reaproveita `ReplaySession`.
