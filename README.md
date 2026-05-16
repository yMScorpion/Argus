# ARGUS

Plataforma de trading cripto: análise técnica e orderflow nível institucional.

> *"See everything. Trade nothing else."*

## Status

**Em construção.** Fundação arquitetural (Fases 0-6 do roadmap). Não usar para trading real.

## Topologia

ARGUS é uma **suíte de quatro processos** isolados por domínio de falha:

| Processo | Responsabilidade | Acesso a credenciais |
|---|---|---|
| **Terminal** | Render, drawings, anotações; emite *intents* | Não |
| **Data Plane** | Ingest, normalize, store, replay, quality scoring | Não |
| **Risk Daemon** | Risk envelope, order routing, reconciliation, kill switch | **Sim, único** |
| **Research** | Backtest, ML, plugin marketplace | Não |

Crash do Terminal **não** mata posições abertas. Crash do Data Plane **não** afeta ordens. Risk Daemon é o componente mais protegido — restart com reconciliação obrigatória antes de aceitar novos intents.

## Roadmap

Veja `docs/roadmap.md` para as 27 fases. Esta versão entrega Fases 0-6:

- **Fase 0** — Operating system de desenvolvimento (workspace, ADRs, CI, ownership)
- **Fase 1** — Tipos fundamentais, schemas, contratos
- **Fase 2** — IPC tipado, SHM SPSC lock-free, health protocol, command bus
- **Fase 3** — Data Plane skeleton com fixture connector, SHM publisher, quality tracker
- **Fase 4** — Orderbook L2 com slab arena + property tests (stress 500-2000 ops)
- **Fase 5** — Storage tiered (hot/JSONL warm), checkpoints, replay determinístico, branching
- **Fase 6** — Métricas (registry + Prometheus exporter), `Sensitive<T>`, JSON structured logging, trace primitives

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo bench --workspace
make ci
```

Toolchain: Rust stable 1.78+.

## Estrutura

```
crates/
  argus-core-types/      # IDs canônicos, instrument, venue
  argus-time/            # clock model (exchange/recv/process)
  argus-decimal/         # PriceTicks/QtyLots/MoneyMinor (i128, sem f64)
  argus-errors/          # taxonomia ArgusError
  argus-capability/      # capability matrix por venue
  argus-provenance/      # provenance tracking
  argus-schema/          # tipos serializáveis (Cap'n Proto migration: ADR-0006)
  argus-shm/             # SPSC ring buffer lock-free
  argus-ipc/             # comando bus tipado
  argus-orderbook/       # L2 com slab arena (não BTreeMap)
  argus-connectors-core/ # VenueConnector trait
  argus-connectors-fixture/ # connector determinístico para testes
  argus-derived/         # CVD, candles, footprint
  argus-data-plane/      # binário Data Plane
  argus-audit/           # log append-only com hash chain
  argus-risk/            # envelope + state machine
  argus-replay/          # replay determinístico com branching
  argus-storage/         # hot tier + JSONL warm tier + checkpoints + segment index
  argus-observability/   # metrics registry, Prometheus exporter, Sensitive, traces

docs/
  architecture/
    adr/                 # decisões arquiteturais
    invariants/          # invariantes não-negociáveis
    process-boundaries.md
    failure-modes.md
    latency-budget.md
  roadmap.md
  ownership.md
```

## Princípios não-negociáveis

1. **Cada processo, um propósito.** Fronteiras de IPC tipadas; falhas isoladas.
2. **Latência mensurada e mostrada.** Decomposição source/internal/execution exposta na UI.
3. **Capability-aware.** L3 não é fingido onde só existe L2.
4. **Provenance always.** Toda métrica carrega fonte/latência/confidence.
5. **Risk antes de feature.** Envelope, kill switch, audit log presentes desde o MVP.
6. **Numeric policy.** `i128` em ticks/lots/money minor para qualquer caminho que toca PnL/risk/execução. `f64` só em render.

## Licença

Proprietário, todos os direitos reservados.
