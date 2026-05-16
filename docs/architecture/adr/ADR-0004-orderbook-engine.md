# ADR-0004 — Orderbook engine: slab arena com índice esparso

## Status

Accepted

## Contexto

A v1 usava `BTreeMap<Decimal, Level>` para cada lado do book. Problemas:

- Cache-unfriendly: walking BTreeMap para iterar níveis ordenados pula entre
  páginas de memória.
- Alocação por insert (cada novo level allocates node).
- Comparação `Decimal` é lenta vs `i64`/`i128`.
- L3 (Hyperliquid) requer estrutura adicional para ordens individuais que
  BTreeMap não provê.
- Snapshot replace = drop entire tree + rebuild = pause perceptível.

Para 200k events/s sustained com P99 < 400µs, BTreeMap não passa.

## Decisão

Cada lado do book é um **`PriceLadder`**:

```rust
struct PriceLadder {
    /// Slab-allocated levels. Level index ≠ price index.
    levels: Slab<Level>,
    /// Sparse map: tick offset relativo a anchor → slab key.
    /// Anchor é mid recente; reanchored periodicamente.
    index: HashMap<i64, usize>,
    /// Ordem por preço (best-first). Mantida sincronizada com `index`.
    ordered: SortedVec<(i128, usize)>, // (px_ticks, slab_key)
    side: Side,
    tick_size_ticks: u64,
}

struct Level {
    px_ticks: i128,
    size_lots: i128,
    order_count: u32,
    last_update_seq: u64,
    /// L3: lista intrusiva de ordens. None em L2.
    orders: Option<IntrusiveOrderList>,
}
```

Apply de delta:
- L2 update at price: `index.get_or_insert(px) -> slab_key`, mutate `Level`,
  re-position em `ordered` se size foi a zero.
- L2 snapshot: trocar arena via `ArcSwap` atomic — readers continuam vendo
  versão antiga até pegarem nova; readers nunca veem estado parcial.
- L3 add/cancel/replace: O(1) lookup por `OrderId`; ajusta level aggregate;
  intrusive list mantém posição na fila.

Para iteração de "best N levels", `ordered` provê ordem direta.

## Alternativas consideradas

- **`BTreeMap<i128, Level>`:** mantemos como **engine de referência apenas em
  testes** para diff property test. Não em produção.
- **Array denso indexado por tick offset:** ótimo para mid-price stable, mas
  cripto move muito; range pode ser enorme; desperdício de memória.
- **B-tree custom:** complexo de implementar bem; não temos a vantagem de
  cardinality alta que justificaria.

## Consequências

### Positivas
- Update O(1) (modulo hash + sort position update).
- Iteração best-N é cache-friendly via `ordered`.
- Snapshot atomic via `ArcSwap` evita pause.
- Pronto para L3 sem refactor.

### Negativas
- Complexidade maior que BTreeMap.
- `unsafe` em intrusive list (L3) — encapsulado em `argus-orderbook` e testado
  com `loom`/MIRI.
- Hash collision em `index` é caso raro mas precisa ser bem testado.

### Neutras
- Memory footprint: ~40 bytes/level + slab overhead. Aceitável.

## Invariantes

- `best_bid().px < best_ask().px` (em modo `Live`; pode crossar transitoriamente
  em `Resyncing`).
- `level.size_lots > 0` para todo level em `index` e `ordered`. Level com
  size = 0 é removido imediatamente.
- `level.last_update_seq` é monotônico por level.
- `index.len() == ordered.len() == número de levels não-vazios na slab`.
- `ordered` está sempre ordenado (best-first: descending para bids, ascending
  para asks).
- Snapshot apply é atomic (visível ou não visível, nunca parcial).

## Testes obrigatórios

- Property test: aplicar sequence aleatória de deltas; comparar resultado com
  `BTreeMap` reference engine; mismatch == bug.
- Property test: snapshot no meio de stream de deltas; engine continua
  consistente.
- Property test: gap detection — sequence skip dispara `BookStatus::GapDetected`
  imediatamente.
- Replay determinístico: mesmo input + mesma versão produz mesmo book final
  (hash do book state).
- Benchmark: 1M updates aleatórios; record P50/P99.
- Audit: invariant check após cada update em `cfg(debug_assertions)`.
