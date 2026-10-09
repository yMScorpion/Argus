# Orderbook — Invariantes

## I-OB-1 — best_bid < best_ask em estado Live
Pode haver crossing transitório em `Resyncing`/`GapDetected`; nunca em `Live`.

## I-OB-2 — Sequence é monotônico por canal
`event.sequence_id` para o mesmo (venue, symbol, channel) só aumenta. Skip > 1
imediatamente dispara `GapDetected`.

## I-OB-3 — Snapshot apply é atomic
Visível ou não visível, nunca parcial. Implementado via `ArcSwap` ou ping-pong
de buffers.

## I-OB-4 — Levels com size = 0 são removidos imediatamente
Não persistem em `index` nem `ordered`. Sweep periodic não é necessário.

## I-OB-5 — `last_update_seq` por level é monotônico
Update com `seq <= last_update_seq` é descartado (com alerta).

## I-OB-6 — `index.len() == ordered.len() == count_non_empty(slab)`
Property test verifica após cada apply.

## I-OB-7 — `ordered` está sempre ordenado
Bids: descending por price. Asks: ascending. Verificado em debug builds após
cada apply.

## I-OB-8 — Tick alignment
Todo `px_ticks` é múltiplo de `tick_size_ticks` (ou tick_size = 1 quando ticks
são unidade primitiva).

## I-OB-9 — Sum size invariant
`sum(level.size_lots)` igual à soma reportada por venue em snapshots
periódicos (modulo deltas in-flight). Discrepância dispara audit.

## I-OB-10 — L3 order count consistency
Quando L3 disponível: `level.order_count == level.orders.len()` e
`sum(order.size) == level.size_lots`.

## I-OB-11 — Status transitions
`Empty → SnapshotLoaded → Live` ⇄ `GapDetected → Resyncing → Live`. Outros
caminhos requerem ADR.

## I-OB-12 — Confidence herda capabilities
Métricas derivadas (footprint, CVD) computadas sobre book em estado
`GapDetected`/`Resyncing` carregam confidence reduzida no provenance.
