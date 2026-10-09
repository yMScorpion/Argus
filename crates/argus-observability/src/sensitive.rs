//! `Sensitive<T>` — wrapper que apaga conteúdo em `Debug`/`Display`.
//!
//! Conforme spec §13.4, secrets nunca devem aparecer em logs. CI deve falhar
//! se algum tipo `Sensitive` for logado diretamente (não temos linter custom
//! ainda, mas o wrapper protege contra todos os caminhos de format).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Wrapper que esconde `T` em representações textuais.
///
/// Acesso ao valor real requer `into_inner()` ou `expose()` — chamadas
/// auditáveis em code review.
#[derive(Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sensitive<T>(T);

impl<T> Sensitive<T> {
    /// Constrói.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Extrai por valor (consome).
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Expõe referência ao valor (use com cuidado).
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<REDACTED:{}>", std::any::type_name::<T>())
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<REDACTED>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_emit_value() {
        let s = Sensitive::new(String::from("super_secret_password_123"));
        let formatted = format!("{s:?}");
        assert!(!formatted.contains("super_secret_password_123"));
        assert!(formatted.contains("REDACTED"));
    }

    #[test]
    fn display_does_not_emit_value() {
        let s = Sensitive::new(42u32);
        let formatted = format!("{s}");
        assert_eq!(formatted, "<REDACTED>");
    }

    #[test]
    fn expose_returns_value() {
        let s = Sensitive::new(String::from("ok"));
        assert_eq!(s.expose(), "ok");
    }

    #[test]
    fn into_inner_consumes() {
        let s = Sensitive::new(String::from("ok"));
        let inner = s.into_inner();
        assert_eq!(inner, "ok");
    }

    #[test]
    fn serde_roundtrip_is_transparent() {
        // Pelo `serde(transparent)`, JSON é o JSON do inner.
        let s = Sensitive::new(123u32);
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, "123");
        let back: Sensitive<u32> = serde_json::from_str(&j).unwrap();
        assert_eq!(back.expose(), &123);
    }
}
