//! Shared memory ring buffer SPSC lock-free.
//!
//! Conforme [ADR-0003], este crate provê:
//!
//! - `SpscRing<T>` — ring SPSC lock-free com slot sequence + memory ordering
//!   `Acquire`/`Release` corretos.
//! - Padding contra false sharing entre producer e consumer.
//! - Backpressure policy declarada (`OverflowPolicy`).
//!
//! # Fase atual vs futuro
//!
//! Esta versão (Fase 2) implementa o algoritmo em **memória do processo**
//! (heap). A migração para **shared memory real** (POSIX `shm_open` / Windows
//! `CreateFileMapping`) é Fase 7 — o algoritmo é o mesmo; apenas a
//! alocação muda.
//!
//! # Algoritmo
//!
//! Para `capacity` potência de 2:
//!
//! - Cada slot tem seu próprio `slot_seq: AtomicU64`.
//! - Producer (single):
//!   1. Lê `slot_seq` do slot `pos & mask` com `Acquire`.
//!   2. Se `slot_seq == pos`, slot disponível para escrita.
//!   3. Escreve payload.
//!   4. Atualiza `slot_seq = pos + 1` com `Release`.
//!   5. Avança `producer_pos` com `Release`.
//! - Consumer (single):
//!   1. Lê `slot_seq` do slot `pos & mask` com `Acquire`.
//!   2. Se `slot_seq == pos + 1`, payload disponível.
//!   3. Lê payload.
//!   4. Atualiza `slot_seq = pos + capacity` com `Release` (libera para
//!      próxima rotação).
//!   5. Avança `consumer_pos` com `Release`.
//!
//! Este padrão (slot sequence) é o mesmo do Disruptor / boost::lockfree::spsc.
//! Detecta corretamente o caso "consumer alcançou producer" (vazio) e
//! "producer alcançou consumer" (cheio).

#![deny(missing_docs)]

mod ring;

pub use ring::{Consumer, OverflowPolicy, Producer, ShmRingError, SpscRing};
