# ADR-0002 — Política numérica: i128 em ticks/lots/money minor

## Status

Accepted

## Contexto

Trading lida com decimais que precisam ser **exatos**. `f64` tem problemas
conhecidos:

- Adição não-associativa: `(0.1 + 0.2) + 0.3 != 0.1 + (0.2 + 0.3)`.
- Comparações via tolerância são fonte de bugs em níveis de orderbook.
- Acumular fees ao longo de milhares de fills produz drift.
- Cálculo de margem em cross/portfolio é numericamente sensível.

PnL contabilizado em float é irrecuperável: trader não pode auditar, contador
não pode reportar, e divergência com a venue é inexplicável.

## Decisão

**Caminho de execução, risco e contabilidade:** apenas inteiros escalados:

- `PriceTicks(i128)` — preço como número inteiro de ticks; tick size declarado
  por instrumento.
- `QtyLots(i128)` — quantidade como número inteiro de lots; lot size declarado.
- `MoneyMinor(i128)` — valores monetários em sub-unidade da moeda (centavos para
  USD, satoshis para BTC, etc).
- `BasisPoints(i32)` — fees, funding rates, slippage.

Conversão decimal → inteiro **exige política de arredondamento explícita**:
- `RoundDown` (default para qty em buy)
- `RoundUp` (default para qty em sell que reduz posição? não — explicit)
- `RoundNearest`
- `RejectIfNotAligned` (para preços que devem cair em tick)

**Caminho de render:** conversão para `f32` permitida apenas em
`argus-render`, na borda do GPU buffer, com `clamp` e `epsilon` documentados.

**Caminho analítico (backtest, indicadores):** `Decimal` (via `rust_decimal`)
ou inteiros escalados, escolha por crate. Nunca `f64` em valores que viram PnL.

## Alternativas consideradas

- **`f64` em todo lugar:** rejeitado pelos motivos acima.
- **`Decimal` (rust_decimal) em todo lugar:** mais ergonômico mas mais lento;
  alocação em algumas operações; orderbook com `BTreeMap<Decimal, _>` foi
  exatamente o anti-pattern da v1 que estamos removendo.
- **`i64` em vez de `i128`:** insuficiente. BTC notional em sats × leverage ×
  conta institucional pode estourar i64 em casos extremos. i128 é abundante
  e tem performance praticamente idêntica em CPUs modernas.

## Consequências

### Positivas
- Aritmética exata, associativa, comutativa.
- Comparação de níveis de orderbook é `==` em ticks, sem tolerância.
- Audit log reproduz PnL bit-a-bit.
- Reconciliação com venue é determinística (modulo arredondamento documentado).

### Negativas
- Conversões `Decimal <-> Ticks` precisam ser explícitas.
- Programadores precisam pensar em tick size ao construir preços.
- Arredondamento equivocado vira bug de risco — política deve ser sempre
  declarada.

### Neutras
- Tipos opaque (`PriceTicks`, `QtyLots`) impedem mistura acidental no compilador.

## Invariantes

- Em crates do Risk Daemon (`argus-risk`, `argus-execution`), zero ocorrências
  de `f64` ou `f32`. CI verifica via grep.
- Em `argus-orderbook`, preços comparados como `i128` em ticks; nunca
  `Decimal` ou float.
- Em `argus-audit`, valores serializados como inteiros escalados + decimal
  string para humanos (nunca como float).

## Testes obrigatórios

- Property test (`argus-decimal`): `decimal -> ticks -> decimal` round-trip
  preserva valor quando alinhado, ou retorna erro/arredondamento explícito.
- Property test: arithmétic ops (add/sub/mul scalar) em `MoneyMinor` são
  associativas e comutativas.
- Property test: conversão de stop price nunca aumenta risco silenciosamente
  (`RoundUp` para sell stop, `RoundDown` para buy stop ⇒ stop sempre mais
  conservador).
- CI grep em `argus-risk/**`, `argus-execution/**`, `argus-orderbook/**` para
  bloquear `f64`/`f32`.
