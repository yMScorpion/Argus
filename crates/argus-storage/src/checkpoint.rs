//! Checkpoints — snapshots periódicos do estado consolidado.
//!
//! Conforme spec §5.4, checkpoint contém:
//!
//! - orderbook state
//! - CVD state (Fase 5.1)
//! - footprint aggregation state (Fase 5.1)
//! - indicator base state (Fase 5.1)
//! - last sequence per channel
//! - schema version
//! - code version
//! - input segment offsets (resume info)
//!
//! Esta versão Fase 5 inicial implementa **orderbook + sequence**. Outros
//! componentes plugam neste mesmo formato adicionando seções opcionais.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use argus_core_types::InstrumentId;

/// Snapshot serializável de um orderbook em um ponto no tempo.
///
/// Stored como bids/asks separados ordenados; reconstruir um `OrderBook`
/// concreto do snapshot é responsabilidade do consumidor (data-plane crate),
/// para evitar dependency cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderBookCheckpointPayload {
    /// Bids como (price_ticks, size_lots), best-first.
    pub bids: Vec<(String, String)>,
    /// Asks como (price_ticks, size_lots), best-first.
    pub asks: Vec<(String, String)>,
    /// Tick size em formato decimal (preservado como string para serde
    /// portabilidade).
    pub tick_size: String,
    /// Lot size.
    pub lot_size: String,
    /// Sequence id do último update aplicado.
    pub last_seq: u64,
}

/// Checkpoint completo de uma instância de Data Plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Versão do schema do checkpoint (incrementa em mudanças incompatíveis).
    pub checkpoint_schema_version: u16,
    /// Versão do código que produziu (semver).
    pub code_version: String,
    /// Versão do schema canonical em uso.
    pub canonical_schema_version: u16,
    /// Monotonic timestamp (nanos).
    pub created_at_ns: u64,
    /// Mapping: instrument → state do orderbook.
    pub orderbooks: std::collections::BTreeMap<String, OrderBookCheckpointPayload>,
    /// Hash blake3 sobre o JSON serializado dos orderbooks (detecta
    /// corrupção de checkpoint).
    pub integrity_hash: [u8; 32],
}

impl Checkpoint {
    /// Constrói novo (sem hash; chame `seal()` para preencher).
    pub fn new(canonical_schema_version: u16, code_version: impl Into<String>, created_at_ns: u64) -> Self {
        Self {
            checkpoint_schema_version: 1,
            code_version: code_version.into(),
            canonical_schema_version,
            created_at_ns,
            orderbooks: std::collections::BTreeMap::new(),
            integrity_hash: [0u8; 32],
        }
    }

    /// Adiciona orderbook serializado.
    pub fn add_orderbook(&mut self, instrument: InstrumentId, payload: OrderBookCheckpointPayload) {
        self.orderbooks.insert(format!("{}", instrument.raw()), payload);
    }

    /// Computa e seta integrity_hash baseado no conteúdo atual.
    pub fn seal(&mut self) -> Result<(), CheckpointError> {
        // Hash do JSON ordenado por key (BTreeMap garante).
        let serialized = serde_json::to_vec(&self.orderbooks)
            .map_err(CheckpointError::Serde)?;
        let h = blake3::hash(&serialized);
        self.integrity_hash.copy_from_slice(h.as_bytes());
        Ok(())
    }

    /// Verifica integridade.
    pub fn verify(&self) -> Result<(), CheckpointError> {
        let serialized = serde_json::to_vec(&self.orderbooks)
            .map_err(CheckpointError::Serde)?;
        let h = blake3::hash(&serialized);
        if h.as_bytes() != &self.integrity_hash {
            return Err(CheckpointError::IntegrityMismatch);
        }
        Ok(())
    }
}

/// Erros de checkpoint.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// IO.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialização falhou.
    #[error("serde error: {0}")]
    Serde(serde_json::Error),
    /// Hash não bate (checkpoint corrompido).
    #[error("checkpoint integrity hash mismatch")]
    IntegrityMismatch,
    /// Schema version incompatível.
    #[error("checkpoint schema version {got} unsupported (this code expects {expected})")]
    UnsupportedSchema {
        /// Versão encontrada.
        got: u16,
        /// Versão esperada.
        expected: u16,
    },
}

impl From<serde_json::Error> for CheckpointError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e)
    }
}

/// Armazenamento de checkpoints em disco.
///
/// Cada checkpoint é um arquivo `checkpoint-{seq:020}.json`. Não sobrescreve;
/// novos checkpoints sempre criam novos arquivos.
#[derive(Debug)]
pub struct CheckpointStore {
    root: PathBuf,
    next_seq: u64,
}

impl CheckpointStore {
    /// Abre store em `root` (cria se não existe). Determina `next_seq` a
    /// partir dos arquivos existentes.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, CheckpointError> {
        let root = root.as_ref().to_owned();
        std::fs::create_dir_all(&root)?;
        let next_seq = scan_max_seq(&root)? + 1;
        Ok(Self { root, next_seq })
    }

    /// Próximo seq.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Salva um checkpoint (deve ter `seal()` chamado antes).
    pub fn write(&mut self, checkpoint: &Checkpoint) -> Result<PathBuf, CheckpointError> {
        let path = self.root.join(format!("checkpoint-{:020}.json", self.next_seq));
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let serialized = serde_json::to_vec_pretty(checkpoint)?;
        f.write_all(&serialized)?;
        f.flush()?;
        f.sync_data()?;
        self.next_seq += 1;
        Ok(path)
    }

    /// Lê o checkpoint mais recente (maior seq), verifica integridade.
    pub fn read_latest(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        let max_seq = scan_max_seq(&self.root)?;
        if max_seq == 0 && !self.root.join("checkpoint-00000000000000000000.json").exists() {
            return Ok(None);
        }
        let path = self.root.join(format!("checkpoint-{max_seq:020}.json"));
        let f = File::open(&path)?;
        let reader = BufReader::new(f);
        let checkpoint: Checkpoint = serde_json::from_reader(reader)?;
        checkpoint.verify()?;
        Ok(Some(checkpoint))
    }

    /// Lista todos os seqs disponíveis.
    pub fn list(&self) -> Result<Vec<u64>, CheckpointError> {
        let mut seqs = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(stripped) = name.strip_prefix("checkpoint-") {
                if let Some(seq_str) = stripped.strip_suffix(".json") {
                    if let Ok(seq) = seq_str.parse::<u64>() {
                        seqs.push(seq);
                    }
                }
            }
        }
        seqs.sort();
        Ok(seqs)
    }
}

fn scan_max_seq(root: &Path) -> Result<u64, CheckpointError> {
    let mut max: u64 = 0;
    let mut found = false;
    if !root.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(stripped) = name.strip_prefix("checkpoint-") {
            if let Some(seq_str) = stripped.strip_suffix(".json") {
                if let Ok(seq) = seq_str.parse::<u64>() {
                    if !found || seq > max {
                        max = seq;
                        found = true;
                    }
                }
            }
        }
    }
    Ok(if found { max } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_checkpoint() -> Checkpoint {
        let mut c = Checkpoint::new(1, "argus-0.1.0", 1234567890);
        c.add_orderbook(
            InstrumentId::from_u128(42),
            OrderBookCheckpointPayload {
                bids: vec![("100".into(), "10".into()), ("99".into(), "20".into())],
                asks: vec![("101".into(), "15".into())],
                tick_size: "0.01".into(),
                lot_size: "0.001".into(),
                last_seq: 999,
            },
        );
        c.seal().unwrap();
        c
    }

    #[test]
    fn seal_and_verify_roundtrip() {
        let c = sample_checkpoint();
        assert!(c.verify().is_ok());
    }

    #[test]
    fn tampering_breaks_integrity() {
        let mut c = sample_checkpoint();
        // Mexe nos orderbooks sem reseal.
        c.orderbooks.values_mut().next().unwrap().last_seq = 999_999;
        assert!(matches!(c.verify(), Err(CheckpointError::IntegrityMismatch)));
    }

    #[test]
    fn store_persists_and_recovers() {
        let dir = tempdir().unwrap();
        let mut store = CheckpointStore::open(dir.path()).unwrap();
        let c = sample_checkpoint();
        store.write(&c).unwrap();
        store.write(&c).unwrap();
        let seqs = store.list().unwrap();
        assert_eq!(seqs.len(), 2);

        // Reabre noutra instância — next_seq deve seguir.
        let store2 = CheckpointStore::open(dir.path()).unwrap();
        assert_eq!(store2.next_seq(), 3);

        // read_latest devolve o mais novo verificado.
        let latest = store2.read_latest().unwrap().expect("checkpoint exists");
        assert_eq!(latest.code_version, c.code_version);
    }

    #[test]
    fn read_latest_returns_none_when_empty() {
        let dir = tempdir().unwrap();
        let store = CheckpointStore::open(dir.path()).unwrap();
        assert!(store.read_latest().unwrap().is_none());
    }
}
