# Latency Budget

Decomposição honesta da hot path. Sem promessas de "sub-ms end-to-end" sobre
internet pública.

## Estágios

| # | Estágio | Faixa típica | P99 alvo ARGUS | Notas |
|---|---|---|---|---|
| 1 | **Venue → nossa borda** (source latency) | 30–250ms | depende da venue | Binance Spot depth: cadence 100/1000ms; Futures: 100/250/500ms. **Fora do nosso controle.** Medido e exibido. |
| 2 | Decode WS frame | 50–300µs | < 200µs P99 | parsing zero-alloc, decompress se aplicável |
| 3 | Normalize → canonical event | 30–150µs | < 120µs P99 | inclui dedup, sequence check, watermark |
| 4 | Orderbook apply | 80–500µs | < 400µs P99 | varia com profundidade afetada |
| 5 | Derive streams (CVD, footprint cell, indicators) | 50–500µs | < 400µs P99 por canal | paralelizado por venue/símbolo |
| 6 | SHM write + signal Terminal | 5–30µs | < 25µs P99 | ring buffer producer write |
| 7 | Terminal read + render queue | 100µs–4ms | < 3ms P99 | depende de frame budget |
| 8 | GPU draw → swap chain present | 4–11ms | < 11ms P99 (90fps) ou < 8ms (120fps) | display refresh limitado |
| 9 | Click → intent emitted | 200µs–2ms | < 1.5ms P99 | hotkey path; mouse adiciona ~5–8ms OS event lag |
| 10 | Risk Daemon validate + sign + REST submit | 1.5–6ms | < 5ms P99 | local processing; rede a parte |
| 11 | Network → venue + venue ACK | 30–250ms+ | depende do trader e da venue | física da rede |

## O que ARGUS controla

Somando estágios internos (excluindo source latency e venue ACK):

- **tick → tela P99 < 18ms** (estágios 2–8)
- **click → submit local P99 < 7ms** (estágios 9–10)

## O que ARGUS expõe ao trader

Banner persistente com **três latências**:

1. **Fresh-since-venue** = `recv_ts - exchange_ts` (média rolling 30s) — estágio 1.
2. **Internal hot path** = `recv_ts → render presented` (P99 rolling 30s) — estágios 2–8.
3. **Round-trip de execução** = `intent_ts → ACK from venue` (EMA) — estágios 9–11.

Cada uma com indicador visual (verde/amarelo/vermelho) baseado em thresholds
configuráveis.

## Per-componente budgets

### Terminal frame budget (target 144fps = 6.94ms)
- Event drain SHM: < 1ms
- Indicator updates: < 1.5ms
- Drawing tools updates: < 1ms
- Render commands: < 2ms
- GPU present: depende do display

### Data Plane per-event (target 200k events/s sustained)
- Decode: < 200µs
- Normalize: < 120µs
- Orderbook apply: < 400µs
- SHM write: < 25µs
- **Total per event: < 800µs P99**

### Risk Daemon intent processing (target < 5ms local)
- Schema validate: < 100µs
- Risk envelope check: < 200µs
- client_order_id gen + sign: < 50µs
- HTTP request build: < 200µs
- TLS handshake: amortizado via connection pool

## Backpressure policy

| Canal | Política | Justificativa |
|---|---|---|
| Raw WS events | NEVER DROP + buffer expansion | dado bruto é fonte de tudo |
| Orderbook updates → SHM | NEVER DROP + producer slowdown | inconsistência de book = bug grave |
| Derived stream (CVD, footprint) | COALESCE em 16ms se Terminal lento | UI pode aceitar resumo |
| Indicators secundários | DEGRADE (skip ticks, recompute periodic) | não-crítico |
| Drawings auto-detect | DEGRADE + queue | re-emite na próxima oportunidade |
| Audit log → disk | NEVER DROP + sync write | regulatório |

Backpressure events são **logados, expostos no telemetry panel, e disparam
alerta visual no Terminal** ("Indicators throttled — last 12s").

## Realtime scheduling

- **Linux:** SCHED_FIFO opcional se rodando com `CAP_SYS_NICE` ou via systemd
  unit. Não obrigatório. Por padrão usa nice -5 + CPU affinity para threads
  críticas.
- **macOS:** QoS classes (User Interactive para render, User Initiated para
  Data Plane hot path, Utility para Research). Sem promessas de hard real-time.
- **Windows:** MMCSS para audio-like priority em render thread;
  THREAD_PRIORITY_HIGHEST para Data Plane hot path.

ARGUS não promete determinismo hard real-time em desktop OS — promete
**engineering disciplinada com budgets mensurados**.
