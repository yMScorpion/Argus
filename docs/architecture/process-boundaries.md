# Process Boundaries

Regras não-negociáveis sobre quem pode falar com quem, com quais payloads, e
sob qual contrato.

## Grafo permitido

```
              ┌─────────────────────────────────────────────┐
              │                                             │
              ▼                                             │
   ┌────────────────────┐    SHM (mkt) +    ┌──────────────────────┐
   │   ARGUS Terminal   │ ──── IPC Cmd ────│  ARGUS Data Plane    │
   │                    │ ◀── SHM events ──│                      │
   └─────────┬──────────┘                   └──────────────────────┘
             │                                        │
             │ IPC Intent                             │ SHM (mkt subset)
             │ + IPC Cmd                              │ + IPC Cmd
             ▼                                        ▼
   ┌────────────────────┐    IPC Cmd      ┌──────────────────────┐
   │  Risk/Exec Daemon  │ ◀────────────── │  Research / ML       │
   │  (credentials)     │                  │  (off hot path)      │
   └─────────┬──────────┘                  └──────────────────────┘
             │
             │ HTTPS / WSS (TLS pinned, signed)
             ▼
   ┌────────────────────┐
   │  Venue REST/WS     │
   └────────────────────┘
```

## Regras

| De | Para | Permitido | Proibido |
|---|---|---|---|
| Terminal | Data Plane | Subscribe, snapshot request, replay control, health query | Push de eventos sintéticos para outros consumers |
| Terminal | Risk Daemon | Submit Intent, query positions/orders, killswitch, mode change | Direct order submit, raw venue calls |
| Terminal | Venue (any) | **Nenhuma comunicação direta** | Tudo |
| Data Plane | Terminal | Stream eventos (SHM), config updates (IPC) | Aceitar ordens |
| Data Plane | Risk Daemon | Stream eventos relevantes (mark price, funding, liq), data quality alerts | Validação de risk |
| Data Plane | Venue (public) | WS subscribe, REST snapshot/historical, health check | Order submit |
| Risk Daemon | Terminal | Order state updates, fill events, reconciliation alerts, killswitch broadcast | Render commands |
| Risk Daemon | Data Plane | User-stream events (forwarded for storage), config | Decisões de chart |
| Risk Daemon | Venue (private) | Order submit/cancel/replace, user data stream, account info | Public market data subscribe (Data Plane already does) |
| Research | Data Plane | Historical query (Parquet/DuckDB), replay session control | Live execution |
| Research | Risk Daemon | Strategy intents (paper or canary modes only) | Bypass envelope |
| Research | Venue | **Nenhuma direta;** tudo via Risk Daemon ou Data Plane | Tudo |

## Crate dependency rules

Enforced by CI grep + `cargo deny`:

```
argus-terminal-* MUST NOT depend on:
  argus-execution
  argus-risk
  argus-connectors-binance (e qualquer venue private)
  argus-research-*

argus-risk MUST NOT depend on:
  tauri
  wgpu
  argus-render
  argus-plugin-runtime
  argus-research-*

argus-data-plane MUST NOT depend on:
  argus-execution
  argus-risk
  tauri
  wgpu

argus-research-* MUST NOT be depended on by:
  argus-data-plane
  argus-risk
  argus-execution
```

## Mensagens

Todo payload IPC carrega:

```rust
struct EnvelopeHeader {
    schema_version: u16,
    sender_process: ProcessRole,
    sender_instance_id: InstanceId,
    monotonic_ts_ns: u64,
    trace_id: TraceId,
    correlation_id: Option<CorrelationId>,
    body_hash: blake3::Hash,
}
```

`body_hash` permite detectar corrupção e adultaração em qualquer link.

## Failure containment

- Crash de Terminal: Risk Daemon detecta heartbeat ausente em 2s. Em modo
  EXECUTE/EXPRESS: continua mantendo posições, bloqueia novos intents. Em
  AUTOMATION: política configurável (default: pause strategy).
- Crash de Data Plane: Terminal mostra banner "DATA STALE"; Risk Daemon entra
  em modo conservador (stops server-side seguem ativos; novos intents
  rejeitados até reconnect + reconciliation).
- Crash de Risk Daemon: alerta resiliente (push mobile + voz se Elite +
  Telegram); auto-restart com reconciliação obrigatória antes de aceitar novos
  intents.
- Crash de Research: nada acontece em produção. Backtests/training perdem
  estado intermediário (checkpointados periodicamente).

## Sem exceção

Se um agente ou contributor sente necessidade de cruzar uma fronteira, ele
**não conserta editando outro domínio**. Abre ADR. A regra-mãe: nada cruza
fronteira de processo sem schema versionado, teste de contrato e política de
falha documentada.
