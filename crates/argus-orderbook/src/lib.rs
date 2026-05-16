//! Orderbook L2 engine com slab arena + índice esparso.
//!
//! Conforme [ADR-0004], evitamos `BTreeMap<Decimal, _>` em favor de:
//!
//! - `Slab<Level>` para arena estável (sem rehash de keys).
//! - `HashMap<i128, usize>` para lookup O(1) por tick price.
//! - `Vec<(i128, usize)>` ordenado para iteração best-first.
//!
//! Comparações de preço são em `i128` (ticks) — não decimal.
//!
//! # Invariantes
//!
//! Ver `docs/architecture/invariants/orderbook.md`. Verificadas em
//! `cfg(debug_assertions)` após cada apply via `OrderBook::assert_invariants`.
//!
//! # Engine de referência
//!
//! O módulo `reference` contém uma implementação `BTreeMap`-based usada
//! apenas em property tests para detectar divergência.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod book;
mod ladder;
pub mod reference;

pub use book::{BookStatus, OrderBook, OrderBookError};
pub use ladder::PriceLadder;
