# ADR-0007 — Serialização textual de `i128` / `u128`

## Status
Accepted (2026-05-15)

## Contexto

ADR-0002 mandata `i128`/`u128` no hot path (preço em ticks, qty em lots,
money em minor units, IDs UUIDv7). Mas `serde_json` — o nosso formato
textual padrão para audit logs, JSONL event store e mensagens IPC de
desenvolvimento — **não suporta esses tipos nativamente**, retornando
`Error("i128 is not supported")` em runtime.

Opções avaliadas:

1. Habilitar `arbitrary_precision` em `serde_json`. Suporta i128 dentro do
   tipo `Value`, mas tem efeitos colaterais (precision em floats, slower
   parsing). Descartado.
2. Usar formato binário (bincode/Cap'n Proto) em todo lugar. Inviável agora:
   audit logs textuais são valor-adicionado (`argus-audit verify` ler em
   qualquer editor).
3. Adapter `with = "..."` de serde que serializa como decimal string.
   **Escolhido.**
4. Adapter UUID-string para u128 IDs. **Escolhido para UUIDv7 IDs.**

## Decisão

- `i128` em `PriceTicks`, `QtyLots`, `MoneyMinor` → serde adapter
  `argus_decimal::i128_str` (decimal string).
- `u128` em IDs UUIDv7 (`InstrumentId`, `IntentId`, `OrderId`,
  `TraceId`, etc) → serde como UUID string (canonical format).
- `u64` IDs (sequencial: `SequenceId`, `EventId`, `SnapshotId`) → `serde
  transparent` (i.e., número JSON nativo; JSON `Number` cobre u64).

Cap'n Proto (Fase 7+) usa os tipos integrais nativos — o adapter aplica só
em formatos textuais.

## Alternativas

Ver acima. A escolha é pragmática: zero perf cost em hot path (binário
nativo), legibilidade em audit/replay/JSON, retro-compatibilidade trivial.

## Consequências

**Boas:**
- Audit logs e replay JSONL lêem em qualquer editor.
- Cap'n Proto futuro não vê diferença (usa i128/u128 direto).
- IDs UUIDv7 mantêm canonical format (URN) facilita correlation cross-tool.

**Ruins:**
- Mensagens JSON são ~10–20 bytes mais largas por campo i128 (string vs
  number). Aceitável para canais textuais.
- Cuidado: nunca usar `as i64` ao desserializar; usar `parse::<i128>`.

## Invariantes

- Round-trip ser→deser de qualquer `PriceTicks`/`QtyLots`/`MoneyMinor` é
  bit-exact.
- IDs UUIDv7 round-trip preservam URN canonical.

## Testes obrigatórios

- `argus_decimal::price::tests::serde_roundtrip` (via canonical event test).
- `argus_core_types::ids::tests::u128_id_v7_monotonic_in_practice`.
- `argus_schema::canonical::tests::serde_roundtrip`.
