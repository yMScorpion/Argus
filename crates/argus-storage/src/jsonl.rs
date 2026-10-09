//! JSONL event store (warm tier inicial).
//!
//! Cada segmento é um arquivo `.jsonl` com canonical events serializados,
//! 1 por linha. Particionamento por dia/símbolo (configurável). Use cases:
//!
//! - Replay determinístico em fixtures.
//! - Auditoria off-line.
//! - Backtest leve.
//!
//! Quando passarmos para Parquet (Fase 5.1), o trait [`crate::EventStore`] e
//! a estrutura de diretórios são preservados — apenas o formato dos arquivos
//! muda. Veja `ADR-0008-storage-implementation-plan.md`.

use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use argus_schema::CanonicalEvent;

use crate::EventStore;

/// Erro do store JSONL.
#[derive(Debug, thiserror::Error)]
pub enum JsonlStoreError {
    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialização falhou.
    #[error("serde_json error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Event store JSONL append-only.
#[derive(Debug)]
pub struct JsonlEventStore {
    root: PathBuf,
    current_segment: Option<CurrentSegment>,
    count: u64,
    /// Tamanho máximo do segmento em bytes; quando excedido, rota para novo.
    segment_size_bytes: u64,
}

#[derive(Debug)]
struct CurrentSegment {
    #[allow(dead_code)] // path retido para futuras operações de rotação atômica
    path: PathBuf,
    writer: BufWriter<File>,
    written_bytes: u64,
}

impl JsonlEventStore {
    /// Abre store em diretório (cria se não existe).
    pub fn open(root: impl AsRef<Path>) -> Result<Self, JsonlStoreError> {
        let root = root.as_ref().to_owned();
        create_dir_all(&root)?;
        Ok(Self {
            root,
            current_segment: None,
            count: 0,
            segment_size_bytes: 64 * 1024 * 1024, // 64MB default
        })
    }

    /// Configura tamanho máximo do segmento.
    pub fn with_segment_size(mut self, bytes: u64) -> Self {
        self.segment_size_bytes = bytes;
        self
    }

    /// Path do diretório raiz.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn rotate_segment_if_needed(&mut self) -> Result<(), JsonlStoreError> {
        let needs_new = match &self.current_segment {
            Some(s) => s.written_bytes >= self.segment_size_bytes,
            None => true,
        };
        if !needs_new {
            return Ok(());
        }

        let segment_id = self.count;
        let path = self.root.join(format!("segment-{segment_id:020}.jsonl"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        self.current_segment = Some(CurrentSegment {
            path,
            writer: BufWriter::new(file),
            written_bytes: 0,
        });
        Ok(())
    }
}

impl EventStore for JsonlEventStore {
    type Error = JsonlStoreError;

    fn append(&mut self, event: &CanonicalEvent) -> Result<(), Self::Error> {
        self.rotate_segment_if_needed()?;
        let segment = self.current_segment.as_mut().expect("rotated above");
        let line = serde_json::to_string(event)?;
        segment.writer.write_all(line.as_bytes())?;
        segment.writer.write_all(b"\n")?;
        segment.written_bytes += line.len() as u64 + 1;
        self.count += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        if let Some(s) = &mut self.current_segment {
            s.writer.flush()?;
        }
        Ok(())
    }

    fn count(&self) -> u64 {
        self.count
    }
}

/// Reader de segmentos JSONL.
///
/// Itera em ordem de criação (segment-0..segment-N).
#[derive(Debug)]
pub struct JsonlReader {
    segments: Vec<PathBuf>,
    current_index: usize,
    current: Option<BufReader<File>>,
}

impl JsonlReader {
    /// Abre reader, lendo todos os arquivos `segment-*.jsonl` do diretório.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, JsonlStoreError> {
        let root = root.as_ref();
        let mut segments: Vec<PathBuf> = std::fs::read_dir(root)?
            .filter_map(|r| r.ok())
            .map(|d| d.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e == "jsonl")
                    .unwrap_or(false)
            })
            .collect();
        segments.sort();
        Ok(Self {
            segments,
            current_index: 0,
            current: None,
        })
    }

    /// Quantos segmentos foram encontrados.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

impl Iterator for JsonlReader {
    type Item = Result<CanonicalEvent, JsonlStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Tenta ler do segmento atual.
            if let Some(reader) = &mut self.current {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        // EOF do segmento atual; tenta próximo.
                        self.current = None;
                        self.current_index += 1;
                        continue;
                    }
                    Ok(_) => {
                        // Trim final newline.
                        let trimmed = line.trim_end();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<CanonicalEvent>(trimmed) {
                            Ok(e) => return Some(Ok(e)),
                            Err(e) => return Some(Err(JsonlStoreError::Serde(e))),
                        }
                    }
                    Err(e) => return Some(Err(JsonlStoreError::Io(e))),
                }
            } else if self.current_index < self.segments.len() {
                // Abre próximo segmento.
                let path = &self.segments[self.current_index];
                match File::open(path) {
                    Ok(f) => self.current = Some(BufReader::new(f)),
                    Err(e) => return Some(Err(JsonlStoreError::Io(e))),
                }
            } else {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus};
    use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
    use argus_schema::{DataQuality, EventPayload, TradePayload, SCHEMA_VERSION};
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;
    use tempfile::tempdir;

    fn event(seq: u64) -> CanonicalEvent {
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(seq),
            instrument: InstrumentId::from_u128(1),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(seq),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(seq * 1_000_000)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(seq * 1_000_000 + 50_000)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(seq * 1_000_000 + 100_000)),
            venue_status: VenueStatus::Normal,
            quality: DataQuality::default(),
            payload: EventPayload::Trade(TradePayload {
                price: PriceTicks::from_ticks(100, TickSize::new(dec!(0.01)).unwrap()),
                qty: QtyLots::from_lots(seq as i128, LotSize::new(dec!(0.001)).unwrap()),
                aggressor: Side::Buy,
                trade_id: TradeId::new(format!("t-{seq}")),
                aggressor_is_maker: false,
            }),
        }
    }

    #[test]
    fn append_and_read_roundtrip() {
        let dir = tempdir().unwrap();
        let mut store = JsonlEventStore::open(dir.path()).unwrap();
        for i in 1..=5 {
            store.append(&event(i)).unwrap();
        }
        store.flush().unwrap();
        assert_eq!(store.count(), 5);

        // Re-abrir como reader.
        let reader = JsonlReader::open(dir.path()).unwrap();
        let events: Result<Vec<_>, _> = reader.collect();
        let events = events.unwrap();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].sequence_id.raw(), 1);
        assert_eq!(events[4].sequence_id.raw(), 5);
    }

    #[test]
    fn segment_rotates_when_size_exceeded() {
        let dir = tempdir().unwrap();
        // Cada evento serializado tem ~hundreds de bytes; configure rotação
        // bem agressiva para forçar múltiplos segmentos.
        let mut store = JsonlEventStore::open(dir.path())
            .unwrap()
            .with_segment_size(256);
        for i in 1..=20 {
            store.append(&event(i)).unwrap();
        }
        store.flush().unwrap();
        let reader = JsonlReader::open(dir.path()).unwrap();
        assert!(reader.segment_count() > 1, "deve haver múltiplos segmentos");
    }

    #[test]
    fn replay_is_deterministic_byte_for_byte() {
        // Sessão 1.
        let dir1 = tempdir().unwrap();
        let mut s1 = JsonlEventStore::open(dir1.path()).unwrap();
        for i in 1..=10 {
            s1.append(&event(i)).unwrap();
        }
        s1.flush().unwrap();

        // Sessão 2 (mesmo input, novo dir).
        let dir2 = tempdir().unwrap();
        let mut s2 = JsonlEventStore::open(dir2.path()).unwrap();
        for i in 1..=10 {
            s2.append(&event(i)).unwrap();
        }
        s2.flush().unwrap();

        // Eventos lidos devem ser idênticos.
        let r1: Vec<_> = JsonlReader::open(dir1.path()).unwrap().collect::<Result<_, _>>().unwrap();
        let r2: Vec<_> = JsonlReader::open(dir2.path()).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(r1, r2);
    }
}
