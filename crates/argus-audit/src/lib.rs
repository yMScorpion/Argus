//! Audit log append-only com hash chain.
//!
//! Conforme invariante I-7 do Risk Daemon, toda decisão (intent recebido,
//! ordem submetida, fill, cancel, kill switch) é gravada com `prev_hash`
//! chained via blake3. Adulteração detectável via `argus-audit verify`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use argus_errors::{ArgusError, Result};
use argus_time::UnixNanos;

/// Hash blake3 de 32 bytes.
pub type Hash = [u8; 32];

/// Entrada de audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Sequence id monotônico por instalação.
    pub seq: u64,
    /// Timestamp (Unix nanos).
    pub ts: UnixNanos,
    /// Tipo de evento.
    pub event_type: String,
    /// Payload serializado (JSON; em fase 7+ migra para Cap'n Proto).
    pub payload_json: String,
    /// Hash da entrada anterior (zeros para a primeira).
    #[serde(with = "hex_bytes")]
    pub prev_hash: Hash,
    /// Hash desta entrada.
    #[serde(with = "hex_bytes")]
    pub this_hash: Hash,
}

impl AuditEntry {
    /// Computa o hash que deveria estar em `this_hash` dado os outros campos.
    pub fn compute_hash(&self) -> Hash {
        let mut h = blake3::Hasher::new();
        h.update(&self.seq.to_le_bytes());
        h.update(&self.ts.as_nanos().to_le_bytes());
        h.update(self.event_type.as_bytes());
        h.update(self.payload_json.as_bytes());
        h.update(&self.prev_hash);
        let out = h.finalize();
        let mut buf = [0u8; 32];
        buf.copy_from_slice(out.as_bytes());
        buf
    }
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        s.serialize_str(&hex)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        if s.len() != 64 {
            return Err(serde::de::Error::custom("expected 64 hex chars"));
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|_| serde::de::Error::custom("invalid hex"))?;
        }
        Ok(out)
    }
}

/// Logger append-only.
///
/// Cada `append` faz: serialize → write → flush. Para garantia regulatória
/// real (NEVER DROP) usa `sync_data` opcional via `with_sync(true)`.
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    writer: BufWriter<std::fs::File>,
    last_hash: Hash,
    next_seq: u64,
    sync: bool,
}

impl AuditLog {
    /// Abre (ou cria) audit log no caminho dado.
    ///
    /// Se arquivo já existe, lê última linha para recuperar `last_hash` e
    /// `next_seq`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let (last_hash, next_seq) = if path.exists() {
            recover_state(&path)?
        } else {
            ([0u8; 32], 0)
        };
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| ArgusError::Storage { message: format!("open audit log: {e}") })?;
        Ok(Self {
            path,
            writer: BufWriter::new(file),
            last_hash,
            next_seq,
            sync: false,
        })
    }

    /// Habilita sync_data após cada append (NEVER DROP).
    pub fn with_sync(mut self, enabled: bool) -> Self {
        self.sync = enabled;
        self
    }

    /// Appenda nova entrada. Retorna o hash gerado.
    pub fn append<T: Serialize>(&mut self, event_type: &str, ts: UnixNanos, payload: &T) -> Result<Hash> {
        let payload_json = serde_json::to_string(payload)
            .map_err(|e| ArgusError::Storage { message: format!("serialize audit payload: {e}") })?;
        let mut entry = AuditEntry {
            seq: self.next_seq,
            ts,
            event_type: event_type.to_string(),
            payload_json,
            prev_hash: self.last_hash,
            this_hash: [0u8; 32],
        };
        entry.this_hash = entry.compute_hash();
        let line = serde_json::to_string(&entry)
            .map_err(|e| ArgusError::Storage { message: format!("serialize audit entry: {e}") })?;
        self.writer
            .write_all(line.as_bytes())
            .map_err(|e| ArgusError::Storage { message: format!("write audit: {e}") })?;
        self.writer
            .write_all(b"\n")
            .map_err(|e| ArgusError::Storage { message: format!("write audit nl: {e}") })?;
        self.writer
            .flush()
            .map_err(|e| ArgusError::Storage { message: format!("flush audit: {e}") })?;
        if self.sync {
            self.writer
                .get_ref()
                .sync_data()
                .map_err(|e| ArgusError::Storage { message: format!("sync audit: {e}") })?;
        }
        self.last_hash = entry.this_hash;
        self.next_seq = self.next_seq.checked_add(1).ok_or(ArgusError::Storage {
            message: "audit seq overflow".into(),
        })?;
        Ok(entry.this_hash)
    }

    /// Path do log.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Próximo seq.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Último hash.
    pub fn last_hash(&self) -> Hash {
        self.last_hash
    }
}

fn recover_state(path: &Path) -> Result<(Hash, u64)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ArgusError::Storage { message: format!("read audit: {e}") })?;
    let mut last_hash = [0u8; 32];
    let mut next_seq = 0u64;
    let mut found = false;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: AuditEntry = serde_json::from_str(line)
            .map_err(|e| ArgusError::Storage { message: format!("parse audit entry: {e}") })?;
        last_hash = entry.this_hash;
        next_seq = entry.seq + 1;
        found = true;
    }
    if !found {
        Ok(([0u8; 32], 0))
    } else {
        Ok((last_hash, next_seq))
    }
}

/// Verifica integridade de um audit log inteiro.
///
/// Retorna `Ok(count)` com número de entries; `Err(...)` se alguma quebrou a
/// chain.
pub fn verify(path: impl AsRef<Path>) -> Result<u64> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ArgusError::Storage { message: format!("read audit: {e}") })?;
    let mut prev_hash = [0u8; 32];
    let mut expected_seq = 0u64;
    let mut count = 0u64;
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: AuditEntry = serde_json::from_str(line).map_err(|e| ArgusError::Storage {
            message: format!("parse line {}: {e}", i + 1),
        })?;
        if entry.seq != expected_seq {
            return Err(ArgusError::Storage {
                message: format!("seq mismatch at line {}: expected {} got {}", i + 1, expected_seq, entry.seq),
            });
        }
        if entry.prev_hash != prev_hash {
            return Err(ArgusError::Storage {
                message: format!("hash chain broken at line {}", i + 1),
            });
        }
        let computed = entry.compute_hash();
        if computed != entry.this_hash {
            return Err(ArgusError::Storage {
                message: format!("entry hash mismatch at line {}", i + 1),
            });
        }
        prev_hash = entry.this_hash;
        expected_seq = entry.seq + 1;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use tempfile::tempdir;

    #[derive(Serialize)]
    struct E {
        msg: String,
        n: u32,
    }

    #[test]
    fn append_and_verify_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let mut log = AuditLog::open(&path).unwrap();
        log.append("intent_received", UnixNanos::from_nanos(100), &E { msg: "a".into(), n: 1 }).unwrap();
        log.append("order_submitted", UnixNanos::from_nanos(200), &E { msg: "b".into(), n: 2 }).unwrap();
        log.append("fill", UnixNanos::from_nanos(300), &E { msg: "c".into(), n: 3 }).unwrap();
        drop(log);
        let count = verify(&path).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn tampering_is_detected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let mut log = AuditLog::open(&path).unwrap();
        log.append("intent_received", UnixNanos::from_nanos(1), &E { msg: "x".into(), n: 1 }).unwrap();
        log.append("order_submitted", UnixNanos::from_nanos(2), &E { msg: "y".into(), n: 2 }).unwrap();
        drop(log);
        // Corromper event_type da segunda entry (string top-level, não escapada
        // dentro do JSON aninhado de payload).
        let content = std::fs::read_to_string(&path).unwrap();
        let tampered = content.replace("order_submitted", "order_modified9");
        assert_ne!(content, tampered, "padrão de corrupção precisa achar match");
        std::fs::write(&path, tampered).unwrap();
        assert!(verify(&path).is_err(), "tampering deve ser detectado");
    }

    #[test]
    fn recovers_state_on_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let mut log = AuditLog::open(&path).unwrap();
        log.append("a", UnixNanos::from_nanos(1), &E { msg: "x".into(), n: 1 }).unwrap();
        log.append("b", UnixNanos::from_nanos(2), &E { msg: "y".into(), n: 2 }).unwrap();
        let last_hash_before = log.last_hash();
        drop(log);
        let log2 = AuditLog::open(&path).unwrap();
        assert_eq!(log2.next_seq(), 2);
        assert_eq!(log2.last_hash(), last_hash_before);
    }
}
