# ADR-0008 — Plano de implementação de storage

## Status
Accepted (2026-05-15)

## Contexto

Spec §6.6 mandata três tiers de storage:

1. **Hot** — RAM ring buffer, últimos ~30 min por símbolo.
2. **Warm** — disco local; **default DuckDB + Parquet**.
3. **Cold** — arquivos Parquet movidos a disco lento / S3.

A v2 substituiu a v1 ("ClickHouse embedded" — termo ambíguo) por
DuckDB+Parquet local-first. Para a Fase 5 (atual) escolhemos uma
implementação incremental que evita pull pesado de deps Arrow/Parquet/DuckDB
enquanto a topologia geral está sendo validada.

## Decisão

Implementação em camadas:

| Camada | Fase | Implementação atual | Implementação final |
|---|---|---|---|
| Hot tier | 5 | `argus_storage::hot::HotStore` (VecDeque) | Arena `Slab` + ring (5.1 quando perfilar mostrar) |
| Warm tier writer | 5 | `argus_storage::jsonl::JsonlEventStore` | `argus_storage::parquet::ParquetEventStore` (5.1) |
| Warm tier reader | 5 | `argus_storage::jsonl::JsonlReader` | `argus_storage::parquet::ParquetReader` (5.1) |
| Warm tier queries | — | iteração linear via `JsonlReader` | DuckDB in-process (5.1) |
| Cold tier | 6.1 | (não implementado) | Parquet em diretório user-config / S3 BYO |
| Segment index | 5 | `SegmentIndex` in-memory sorted | B-tree em disco (6+) |
| Checkpoint | 5 | JSON com blake3 integrity | Mesmo, mas com Parquet payload (5.1) |
| Replay | 5 | `ReplaySession` sobre `JsonlReader` | Idem sobre Parquet/DuckDB |

O contrato `EventStore` (trait) é estável e funciona com ambos backends.
Migração JSONL → Parquet é trocar `Box<dyn EventStore>`, sem mudar consumers.

**Por que JSONL primeiro:**

1. Zero dep extra (já usamos serde_json para audit).
2. Determinismo bit-for-bit fácil de provar (`verify_determinism`).
3. Cap. de debug trivial (`less segment-*.jsonl`).
4. Replay de fixtures pequenos (~MB) tem perf aceitável.

**Quando migrar para Parquet:**

- Workloads de horizontes > 7 dias com 10+ símbolos.
- Necessidade de queries analíticas (CVD agregado histórico, footprint
  retrospectivo).
- Compactação importa (Parquet+ZSTD: ~5-10× JSONL).

## Alternativas

- ClickHouse embedded server gerenciado: rejeitado, complexidade
  operacional + heap externo.
- chDB (ClickHouse engine in-process): considerado para Fase 5.1 como
  opção paralela ao DuckDB.

## Consequências

**Boas:**
- Fase 5 entrega valor concreto (replay determinístico) sem bloquear em
  decisão de backend final.
- API estável (`EventStore` trait) → migração futura é transparente.
- ADR documenta o débito técnico explicitamente.

**Ruins:**
- JSONL desperdiça disco em produção (mas Fase 5 não é produção).
- Falta query SQL — backtest fica limitado a iteração.

## Invariantes

- `EventStore::append` é idempotente do ponto de vista do schema: re-inserir
  evento com mesmo (seq, exchange_ts) não corrompe replay.
- `JsonlReader` enumera em ordem de segment id; replay re-sort por
  exchange_ts garante determinismo independente de ordem de inserção.
- Checkpoints são imutáveis: novo checkpoint = novo arquivo; substituir
  proibido.

## Testes obrigatórios

- `argus_storage::jsonl::tests::replay_is_deterministic_byte_for_byte`.
- `argus_storage::checkpoint::tests::tampering_breaks_integrity`.
- `argus_replay::session::tests::replay_is_deterministic_via_verify`.

## Próximas iterações

- **5.1**: Adicionar `ParquetEventStore` (parquet crate). Manter JSONL como
  formato de fixtures pequenos.
- **5.2**: DuckDB query engine para backtest.
- **6.1**: Cold tier S3 + transparent rehydrate.
