# ADR-0003 — Shared memory ring buffer SPSC lock-free para hot path

## Status

Accepted

## Contexto

Data Plane → Terminal precisa entregar 200k+ eventos/segundo por símbolo com
latência P99 < 25µs no SHM publish. Mecanismos rejeitados:

- **TCP/UDS local:** syscalls + copy + scheduling jitter. P99 fácil > 100µs.
- **gRPC local:** serialização repetida + alocação. Inaceitável.
- **mpsc channel (tokio/std):** alocação por send; contention em consumer wakeup.
- **`crossbeam::channel`:** ótimo, mas ainda aloca; não atravessa fronteiras de
  processo.

Precisamos de zero-copy real entre processos nativos, com garantias de memória
ordering corretas.

## Decisão

Implementar **SPSC ring buffer lock-free em shared memory** (POSIX `shm_open` +
`mmap` no Linux/macOS; `CreateFileMapping` no Windows):

- Single Producer, Single Consumer por canal. Multi-canal == múltiplos rings.
- Slots de tamanho fixo, alinhados a cache line (64 bytes).
- Header com `write_seq` e `read_seq` em `AtomicU64`, padded para evitar
  false sharing entre produtor/consumidor.
- Producer:
  1. Lê `read_seq` com `Acquire`.
  2. Verifica espaço (`write_seq - read_seq < capacity`).
  3. Escreve payload no slot `write_seq % capacity`.
  4. Faz `write_seq.fetch_add(1, Release)`.
- Consumer:
  1. Lê `write_seq` com `Acquire`.
  2. Se `read_seq < write_seq`, lê slot `read_seq % capacity`.
  3. Faz `read_seq.fetch_add(1, Release)`.

Slots têm sequence individual também, para detectar leitura antes de escrita
completa (defesa contra reordenação que o compilador permitiria).

Capacidade dimensionada para **30 segundos de burst headroom** por canal.

## Alternativas consideradas

- **MPMC (`crossbeam::queue::ArrayQueue`-style):** mais geral, mais lento, mais
  complexo de provar correto. Não precisamos: cada canal tem 1 produtor (uma
  thread do Data Plane que processa aquele venue/symbol/channel) e 1 consumidor
  (Terminal subscriber). Multi-Terminal usa múltiplos rings ou broadcast layer
  acima.
- **MPSC:** seria útil para audit log, mas audit log usa sync write a disk
  (NEVER DROP por requisito regulatório), não SHM.
- **Lock-based ring (mutex):** lock contention sob carga alta produz tail
  latencies inaceitáveis.

## Consequências

### Positivas
- Hot path latência no nível de instrução (não syscall).
- Zero alocação por evento.
- Zero serialização para tipos POD (Plain Old Data).
- Crash do consumer não corrompe producer (ring é apenas memória).

### Negativas
- `unsafe` necessário (acesso a memória mapeada).
- Ordering bugs em código `unsafe` são notoriamente difíceis de debugar.
- Crash do producer pode deixar estado parcial num slot — consumer precisa
  detectar via slot sequence.
- Tamanhos de slots fixos: tipos com tamanho variável precisam estratégia
  separada (segment ring com tamanhos variáveis, ou index ring + arena).

### Neutras
- Schemas Cap'n Proto / formatos POD funcionam bem; tipos com `Vec` interno
  não — usamos representação `repr(C)` plana.

## Invariantes

- Cada slot tem `slot_seq` próprio. Consumer só lê quando `slot_seq ==
  expected_seq` com ordering `Acquire`.
- Producer escreve payload **antes** de incrementar `slot_seq` (Release).
- Consumer copia payload **antes** de incrementar `read_seq` (Release).
- Capacity é potência de 2; índice via bitmask, não modulo.
- `write_seq` e `read_seq` em cache lines separadas (padding `[u8; 56]` antes
  e depois).

## Testes obrigatórios

- Property test concorrente: spawn produtor + consumidor; envia N elementos;
  verifica que consumidor recebeu todos em ordem, sem duplicação, sem perda.
- Loom test (modelo de memória): pequeno modelo do ring para verificação
  exaustiva de ordering.
- Stress test: produtor enche ring, consumidor lento, verifica `dropped_count`
  conta correto e nenhum slot é lido com `slot_seq` errado.
- Benchmark: latência publish-to-read em mesma thread (memory ops apenas) e
  cross-thread (com `tokio::task::yield_now`).
- ASAN/TSAN/MSAN no CI quando rodando crate `argus-shm`.
