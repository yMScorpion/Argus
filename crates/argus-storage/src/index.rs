//! Segment index — mapeia `(symbol, timestamp)` → `(segment_id, offset)` para
//! seek O(log n) em horizontes históricos.
//!
//! Esta versão é simples: vector ordenado por (symbol, exchange_ts), busca
//! binária. Em Fase 5.1 vira B-tree em disco quando o índice ultrapassar RAM.

use std::cmp::Ordering;

use argus_core_types::InstrumentId;

/// Entry do índice apontando para um evento dentro de um segmento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentIndexEntry {
    /// Instrument (combina venue+symbol+contract type).
    pub instrument: InstrumentId,
    /// Exchange timestamp do evento (nanos desde Unix epoch).
    pub exchange_ts_ns: u64,
    /// ID monotônico do segmento.
    pub segment_id: u64,
    /// Offset (em bytes) dentro do segmento.
    pub byte_offset: u64,
}

impl SegmentIndexEntry {
    /// Chave de ordenação canônica.
    fn key(&self) -> (InstrumentId, u64) {
        (self.instrument, self.exchange_ts_ns)
    }
}

/// Índice in-memory sorted por (instrument, ts).
#[derive(Debug, Default)]
pub struct SegmentIndex {
    entries: Vec<SegmentIndexEntry>,
    sorted: bool,
}

impl SegmentIndex {
    /// Constrói índice vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adiciona entry (não força ordenação imediata).
    pub fn add(&mut self, entry: SegmentIndexEntry) {
        self.entries.push(entry);
        self.sorted = false;
    }

    /// Força ordenação (após batch de inserts).
    pub fn finalize(&mut self) {
        if !self.sorted {
            self.entries.sort_by(|a, b| {
                a.key().0.raw().cmp(&b.key().0.raw()).then(a.key().1.cmp(&b.key().1))
            });
            self.sorted = true;
        }
    }

    /// Busca a entry mais próxima ≤ target. Retorna `None` se nenhuma entry
    /// de `instrument` tem `ts ≤ target`.
    ///
    /// Pré-condição: chamar `finalize()` antes.
    pub fn seek(&self, instrument: InstrumentId, target_ts_ns: u64) -> Option<&SegmentIndexEntry> {
        assert!(self.sorted, "call finalize() before seek()");
        // Busca binária por (instrument, target_ts).
        let target = (instrument, target_ts_ns);
        let result = self.entries.binary_search_by(|e| {
            match e.key().0.raw().cmp(&target.0.raw()) {
                Ordering::Equal => e.key().1.cmp(&target.1),
                other => other,
            }
        });
        let idx = match result {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        // Garante que o resultado é do mesmo instrument.
        let entry = &self.entries[idx];
        if entry.instrument == instrument {
            Some(entry)
        } else {
            // O insert position pode ter caído logo após outra instrument.
            None
        }
    }

    /// Itera entries em ordem (após finalize).
    pub fn iter(&self) -> impl Iterator<Item = &SegmentIndexEntry> + '_ {
        self.entries.iter()
    }

    /// Quantas entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Vazio?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(instrument: u128, ts: u64, seg: u64, off: u64) -> SegmentIndexEntry {
        SegmentIndexEntry {
            instrument: InstrumentId::from_u128(instrument),
            exchange_ts_ns: ts,
            segment_id: seg,
            byte_offset: off,
        }
    }

    #[test]
    fn seek_finds_closest_le() {
        let mut idx = SegmentIndex::new();
        idx.add(entry(1, 1000, 0, 0));
        idx.add(entry(1, 2000, 0, 100));
        idx.add(entry(1, 3000, 1, 0));
        idx.finalize();

        let e = idx.seek(InstrumentId::from_u128(1), 2500).unwrap();
        assert_eq!(e.exchange_ts_ns, 2000);
    }

    #[test]
    fn seek_returns_none_before_first() {
        let mut idx = SegmentIndex::new();
        idx.add(entry(1, 1000, 0, 0));
        idx.finalize();
        assert!(idx.seek(InstrumentId::from_u128(1), 500).is_none());
    }

    #[test]
    fn seek_isolates_per_instrument() {
        let mut idx = SegmentIndex::new();
        idx.add(entry(1, 1000, 0, 0));
        idx.add(entry(2, 2000, 1, 0));
        idx.finalize();
        // Buscar instrument 1 com target alto não pode retornar entry de instrument 2.
        let r1 = idx.seek(InstrumentId::from_u128(1), 5000).unwrap();
        assert_eq!(r1.instrument.raw(), 1);
        let r2 = idx.seek(InstrumentId::from_u128(2), 5000).unwrap();
        assert_eq!(r2.instrument.raw(), 2);
    }

    #[test]
    fn empty_index_seeks_none() {
        let mut idx = SegmentIndex::new();
        idx.finalize();
        assert!(idx.seek(InstrumentId::from_u128(1), 0).is_none());
    }
}
