# Fase 5 — Invariantes

## Storage

- **I-S1: Append is durable after flush.** Após `JsonlEventStore::flush()`,
  o evento aparece em `JsonlReader::open()` em qualquer processo.
- **I-S2: Segments are immutable.** Uma vez fechado (por rotação ou drop),
  segment file não é reescrito. Novos eventos vão para novo segment.
- **I-S3: Read order preserves chronology after sort.** `JsonlReader`
  enumera em ordem de segment id; consumers que precisam ordering por
  `exchange_ts` devem ordenar (replay já faz).
- **I-S4: Hot store overflow drops oldest, never silently corrupts.**
  Estatística `overflowed` é exposta; consumer notado via métrica.

## Checkpoint

- **I-C1: Integrity hash binds payload.** `Checkpoint::seal()` computa
  blake3 sobre o JSON canonical de `orderbooks`. Mutação subsequente
  detectada por `verify()`.
- **I-C2: Checkpoints são imutáveis.** `CheckpointStore::write` usa
  `create_new` — falha se o seq já existe.
- **I-C3: `next_seq` é monotônico.** Reabrir store recupera próximo seq do
  filesystem; nunca recicla.

## Replay

- **I-R1: Replay é determinístico.** `verify_determinism(root, expected)`
  passa se e somente se a sessão produz output bit-for-bit igual.
- **I-R2: Seek é idempotente.** `seek(t); seek(t)` deixa cursor em `t`.
- **I-R3: Branching não muta market data.** Um branch só armazena overlay
  (decisions, paper orders); o storage subjacente é compartilhado.
- **I-R4: Cursor é monotônico dentro de uma execução.** `advance_to(t)`
  pode emitir eventos com ts ≤ t mas não retrocede.

## Segment index

- **I-X1: `seek(instrument, target)` retorna entry com `ts ≤ target` mesmo
  instrument.** Nunca cruza para outro instrument.
- **I-X2: `finalize` é idempotente.** Chamar duas vezes não altera ordem.
- **I-X3: Binary search assume `finalize` foi chamado.** Pré-condição
  enforced via assert.

## Quebra dessas invariantes = bug crítico

Property tests rodam em CI; regressão deve falhar PR.
