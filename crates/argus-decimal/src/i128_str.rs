//! Serde adapter para `i128` como string.
//!
//! `serde_json` (e vários outros formatos) não suportam `i128` nativamente.
//! Como ADR-0002 mandata `i128` no hot path, serializamos como string em
//! formatos textuais e mantemos o número original em formatos binários.
//!
//! Formato binário (Cap'n Proto, bincode) pode usar `i128` direto — este
//! adapter é compatível porque `as_str → parse` é determinístico.

use serde::{Deserialize, Deserializer, Serializer};

/// Serializa `i128` como decimal string.
pub(crate) fn serialize<S: Serializer>(v: &i128, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(v)
}

/// Desserializa `i128` de decimal string.
pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i128, D::Error> {
    let s = String::deserialize(d)?;
    s.parse::<i128>()
        .map_err(|e| serde::de::Error::custom(format!("invalid i128 string: {e}")))
}
