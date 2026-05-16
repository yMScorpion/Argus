# Fase 5 — Storage / Replay base

## Objetivo

Construir persistência durável de eventos canônicos suficiente para suportar
**replay determinístico** e backtest leve, antes da UI avançada ou dos
connectors reais.

Sem replay determinístico, indicadores e drawings ficam frágeis: bug
encontrado em fixture x não reproduz em fixture y.

## Escopo entregue

- `argus-storage` crate com:
  - `HotStore` (RAM, VecDeque, overflow evict-oldest)
  - `JsonlEventStore` (warm tier inicial; rotação por tamanho de segmento)
  - `JsonlReader` (iterador serial sobre segmentos)
  - `SegmentIndex` (sorted in-memory seek O(log n))
  - `Checkpoint` + `CheckpointStore` (snapshots imutáveis hash-verified)
  - Trait `EventStore` para abstrair backends

- `argus-replay` expandido com:
  - `ReplaySession` (open, advance_to, seek)
  - `BranchOverlay` (decisões e paper orders sem duplicar market data)
  - `verify_determinism` (hash bit-for-bit do replay)

## Não-escopo

- Backend Parquet/DuckDB → Fase 5.1 (ADR-0008).
- Cold tier S3 → Fase 6.1.
- Snapshots periódicos automáticos → Fase 8 (precisa connector real).
- Replay UI → Fase 16.

## Critérios de pronto

- ✅ `JsonlEventStore::append` + `JsonlReader` round-trip preserva eventos
  bit-for-bit.
- ✅ Múltiplos segmentos rotacionam automaticamente.
- ✅ Checkpoint corrompido é detectado por blake3.
- ✅ `ReplaySession::verify_determinism` detecta divergência.
- ✅ Branching não duplica market data, apenas decisões/orders.
- ✅ Todos os testes passam (`cargo test -p argus-storage -p argus-replay`).
