use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use argus_core_types::{ReplaySessionId, TraceId};
use argus_schema::CanonicalEvent;
use argus_storage::{JsonlReader, JsonlStoreError};
use argus_time::UnixNanos;

/// Erros possíveis durante replay.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// Storage error.
    #[error("storage error: {0}")]
    Storage(#[from] JsonlStoreError),
    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Sessão não encontrada.
    #[error("replay session {0:?} not found")]
    SessionNotFound(ReplaySessionId),
    /// Schema do checkpoint não suportado.
    #[error("checkpoint schema {got} not supported (expected {expected})")]
    UnsupportedSchema {
        /// Versão encontrada.
        got: u16,
        /// Versão esperada.
        expected: u16,
    },
    /// Replay output diverge do expected (used by verify).
    #[error("replay divergence at event seq {seq}: {reason}")]
    Divergence {
        /// Sequence em que divergiu.
        seq: u64,
        /// Texto livre.
        reason: String,
    },
}

/// Decisão tomada durante replay (manual ou strategy).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayDecision {
    /// ID único.
    pub decision_id: TraceId,
    /// Unix nanos quando a decisão foi tomada (no clock do replay).
    pub at_ns: u64,
    /// Texto livre (rationale, anotação).
    pub note: String,
}

/// Paper order emitida durante replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayPaperOrder {
    /// Order id local da sessão.
    pub order_id: u64,
    /// Quando foi emitida.
    pub at_ns: u64,
    /// Símbolo.
    pub symbol: String,
    /// Side (buy/sell).
    pub side: String,
    /// Quantidade em string (preserva precisão; arrendondamento fica para o
    /// consumer/risk daemon).
    pub qty: String,
    /// Preço (None = market).
    pub price: Option<String>,
}

/// Overlay leve para branching — não duplica market data, só guarda
/// divergências.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BranchOverlay {
    /// ID do branch.
    pub branch_id: u64,
    /// Branch pai (None = root).
    pub parent_branch_id: Option<u64>,
    /// Timestamp em que o branch foi criado.
    pub fork_at_ns: u64,
    /// Decisões tomadas no branch.
    pub decisions: Vec<ReplayDecision>,
    /// Paper orders.
    pub paper_orders: Vec<ReplayPaperOrder>,
}

/// Sessão de replay sobre eventos persistidos.
///
/// Uma sessão lê de um `argus_storage::JsonlEventStore`, e mantém posição
/// monotônica via `cursor_ns`. Branches são overlays opcionais.
#[derive(Debug)]
pub struct ReplaySession {
    session_id: ReplaySessionId,
    storage_root: PathBuf,
    cursor_ns: u64,
    branches: BTreeMap<u64, BranchOverlay>,
    /// Buffer pre-fetched de eventos ordenados; carregado on-demand.
    pending: Vec<CanonicalEvent>,
    pending_idx: usize,
    /// Versão do código que gerou os eventos (para verificar replay
    /// determinístico bit-for-bit).
    code_version: String,
}

impl ReplaySession {
    /// Abre sessão sobre `storage_root`. `start_ns` define o ponto de partida.
    pub fn open(
        session_id: ReplaySessionId,
        storage_root: impl AsRef<Path>,
        start_ns: UnixNanos,
        code_version: impl Into<String>,
    ) -> Result<Self, ReplayError> {
        let path = storage_root.as_ref().to_owned();
        Ok(Self {
            session_id,
            storage_root: path,
            cursor_ns: start_ns.as_nanos(),
            branches: BTreeMap::new(),
            pending: Vec::new(),
            pending_idx: 0,
            code_version: code_version.into(),
        })
    }

    /// Session ID.
    pub fn id(&self) -> ReplaySessionId {
        self.session_id
    }

    /// Code version expectada.
    pub fn code_version(&self) -> &str {
        &self.code_version
    }

    /// Posição atual do cursor.
    pub fn cursor_ns(&self) -> u64 {
        self.cursor_ns
    }

    /// Carrega eventos do storage para o buffer. Filtra `exchange_ts >=
    /// cursor`. Ordena por exchange_ts.
    fn load_pending(&mut self) -> Result<(), ReplayError> {
        let reader = JsonlReader::open(&self.storage_root)?;
        let mut buf: Vec<CanonicalEvent> = Vec::new();
        for e in reader {
            let ev = e?;
            if ev.exchange_ts.as_nanos() >= self.cursor_ns {
                buf.push(ev);
            }
        }
        buf.sort_by_key(|e| e.exchange_ts.as_nanos());
        self.pending = buf;
        self.pending_idx = 0;
        Ok(())
    }

    /// Avança o cursor lendo até `target_ns` (inclusivo). Retorna lista de
    /// eventos passados em ordem.
    pub fn advance_to(&mut self, target_ns: UnixNanos) -> Result<Vec<CanonicalEvent>, ReplayError> {
        if self.pending.is_empty() {
            self.load_pending()?;
        }
        let mut emitted = Vec::new();
        while self.pending_idx < self.pending.len() {
            let ev = &self.pending[self.pending_idx];
            if ev.exchange_ts.as_nanos() > target_ns.as_nanos() {
                break;
            }
            emitted.push(ev.clone());
            self.pending_idx += 1;
        }
        if let Some(last) = emitted.last() {
            self.cursor_ns = last.exchange_ts.as_nanos();
        }
        Ok(emitted)
    }

    /// Faz seek para `target_ns`, descartando state acumulado.
    pub fn seek(&mut self, target_ns: UnixNanos) -> Result<(), ReplayError> {
        self.cursor_ns = target_ns.as_nanos();
        self.pending.clear();
        self.pending_idx = 0;
        Ok(())
    }

    /// Cria um novo branch (overlay) a partir do estado atual.
    pub fn fork(&mut self, branch_id: u64) -> &mut BranchOverlay {
        let overlay = BranchOverlay {
            branch_id,
            parent_branch_id: None,
            fork_at_ns: self.cursor_ns,
            decisions: Vec::new(),
            paper_orders: Vec::new(),
        };
        self.branches.entry(branch_id).or_insert(overlay)
    }

    /// Acesso a branch (read).
    pub fn branch(&self, branch_id: u64) -> Option<&BranchOverlay> {
        self.branches.get(&branch_id)
    }

    /// Acesso a branch (write).
    pub fn branch_mut(&mut self, branch_id: u64) -> Option<&mut BranchOverlay> {
        self.branches.get_mut(&branch_id)
    }

    /// Verifica determinismo: replay completo do storage gera output cujo
    /// hash bate com `expected_hash` (blake3 sobre canonical JSON dos
    /// eventos em ordem).
    pub fn verify_determinism(
        storage_root: impl AsRef<Path>,
        expected_hash: &[u8; 32],
    ) -> Result<(), ReplayError> {
        let reader = JsonlReader::open(storage_root)?;
        let mut hasher = blake3_hasher();
        let mut events: Vec<CanonicalEvent> = Vec::new();
        for e in reader {
            events.push(e?);
        }
        events.sort_by_key(|e| (e.exchange_ts.as_nanos(), e.sequence_id.raw()));
        for (idx, ev) in events.iter().enumerate() {
            let s = serde_json::to_string(ev).map_err(|e| ReplayError::Divergence {
                seq: idx as u64,
                reason: format!("serde error: {e}"),
            })?;
            hasher.update(s.as_bytes());
        }
        let got = hasher.finalize();
        if got.as_bytes() != expected_hash {
            return Err(ReplayError::Divergence {
                seq: events.len() as u64,
                reason: "final hash mismatch".into(),
            });
        }
        Ok(())
    }
}

fn blake3_hasher() -> blake3::Hasher {
    blake3::Hasher::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{
        EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus,
    };
    use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
    use argus_schema::{DataQuality, EventPayload, TradePayload, SCHEMA_VERSION};
    use argus_storage::{EventStore, JsonlEventStore};
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;
    use tempfile::tempdir;

    fn event(seq: u64, ts_ns: u64) -> CanonicalEvent {
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(seq),
            instrument: InstrumentId::from_u128(1),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(seq),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(ts_ns)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(ts_ns + 100)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(ts_ns + 200)),
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

    fn write_fixture_events(root: &Path, events: &[CanonicalEvent]) {
        let mut store = JsonlEventStore::open(root).unwrap();
        for e in events {
            store.append(e).unwrap();
        }
        store.flush().unwrap();
    }

    #[test]
    fn advance_emits_events_up_to_target() {
        let dir = tempdir().unwrap();
        let events: Vec<_> = (1..=5).map(|i| event(i, i * 1_000_000)).collect();
        write_fixture_events(dir.path(), &events);

        let mut sess = ReplaySession::open(
            ReplaySessionId::from_u128(1),
            dir.path(),
            UnixNanos::from_nanos(0),
            "argus-test-0.1.0",
        )
        .unwrap();

        let emitted = sess.advance_to(UnixNanos::from_nanos(3_000_000)).unwrap();
        assert_eq!(emitted.len(), 3);
        assert_eq!(emitted[0].sequence_id.raw(), 1);
        assert_eq!(emitted[2].sequence_id.raw(), 3);
        assert_eq!(sess.cursor_ns(), 3_000_000);
    }

    #[test]
    fn seek_resets_cursor() {
        let dir = tempdir().unwrap();
        let events: Vec<_> = (1..=5).map(|i| event(i, i * 1_000_000)).collect();
        write_fixture_events(dir.path(), &events);

        let mut sess = ReplaySession::open(
            ReplaySessionId::from_u128(1),
            dir.path(),
            UnixNanos::from_nanos(0),
            "argus-test-0.1.0",
        )
        .unwrap();
        sess.advance_to(UnixNanos::from_nanos(2_000_000)).unwrap();
        sess.seek(UnixNanos::from_nanos(4_000_000)).unwrap();
        let emitted = sess.advance_to(UnixNanos::from_nanos(5_000_000)).unwrap();
        assert_eq!(emitted.len(), 2); // events 4, 5
    }

    #[test]
    fn branching_does_not_duplicate_market_data() {
        let dir = tempdir().unwrap();
        let events: Vec<_> = (1..=3).map(|i| event(i, i * 1_000_000)).collect();
        write_fixture_events(dir.path(), &events);

        let mut sess = ReplaySession::open(
            ReplaySessionId::from_u128(1),
            dir.path(),
            UnixNanos::from_nanos(0),
            "argus-test-0.1.0",
        )
        .unwrap();
        sess.advance_to(UnixNanos::from_nanos(2_000_000)).unwrap();
        let _b = sess.fork(7);
        let branch = sess.branch_mut(7).unwrap();
        branch.decisions.push(ReplayDecision {
            decision_id: TraceId::from_u128(99),
            at_ns: 2_000_000,
            note: "long signal".into(),
        });
        branch.paper_orders.push(ReplayPaperOrder {
            order_id: 1,
            at_ns: 2_000_000,
            symbol: "BTCUSDT".into(),
            side: "buy".into(),
            qty: "0.001".into(),
            price: None,
        });

        assert_eq!(sess.branch(7).unwrap().decisions.len(), 1);
        assert_eq!(sess.branch(7).unwrap().paper_orders.len(), 1);
    }

    #[test]
    fn replay_is_deterministic_via_verify() {
        let dir = tempdir().unwrap();
        let events: Vec<_> = (1..=10).map(|i| event(i, i * 1_000_000)).collect();
        write_fixture_events(dir.path(), &events);

        // Computa hash de referência uma vez.
        let reader = JsonlReader::open(dir.path()).unwrap();
        let mut all: Vec<CanonicalEvent> = reader.collect::<Result<_, _>>().unwrap();
        all.sort_by_key(|e| (e.exchange_ts.as_nanos(), e.sequence_id.raw()));
        let mut h = blake3::Hasher::new();
        for ev in &all {
            h.update(serde_json::to_string(ev).unwrap().as_bytes());
        }
        let expected: [u8; 32] = h.finalize().as_bytes().to_owned();

        ReplaySession::verify_determinism(dir.path(), &expected).unwrap();

        // Mutar 1 evento e checar que verify falha.
        // (Recriamos com modificação leve.)
        let dir2 = tempdir().unwrap();
        let mut events2: Vec<_> = events.clone();
        events2[5].sequence_id = SequenceId::from_raw(99999);
        write_fixture_events(dir2.path(), &events2);
        let res = ReplaySession::verify_determinism(dir2.path(), &expected);
        assert!(matches!(res, Err(ReplayError::Divergence { .. })));
    }
}
