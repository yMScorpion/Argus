use serde::{Deserialize, Serialize};
use std::fmt;

/// Erro de manipulação de timestamps.
#[derive(Debug, thiserror::Error)]
pub enum TimestampError {
    /// Subtração que resultaria em valor negativo (i.e., `b > a` quando
    /// computando `a - b`).
    #[error("timestamp subtraction underflow: lhs={lhs} < rhs={rhs}")]
    Underflow {
        /// Lado esquerdo da subtração.
        lhs: u64,
        /// Lado direito da subtração.
        rhs: u64,
    },
    /// Conversão de chrono falhou.
    #[error("invalid datetime: {0}")]
    Invalid(String),
}

/// Wrapper opaco para timestamp em nanosegundos desde Unix epoch.
///
/// Usar `UnixNanos` em vez de `u64` cru previne mistura acidental com
/// timestamps monotônicos ou com timestamps em outras unidades (ms, µs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixNanos(pub u64);

impl UnixNanos {
    /// Constrói a partir de inteiro cru.
    pub const fn from_nanos(v: u64) -> Self {
        Self(v)
    }

    /// Inteiro cru.
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Constrói a partir de milissegundos desde Unix epoch.
    pub const fn from_millis(ms: u64) -> Self {
        Self(ms.saturating_mul(1_000_000))
    }

    /// Constrói a partir de microssegundos desde Unix epoch.
    pub const fn from_micros(us: u64) -> Self {
        Self(us.saturating_mul(1_000))
    }

    /// Diferença em nanos para outro `UnixNanos` (rhs).
    ///
    /// Erro se `self < rhs`.
    pub fn checked_sub(self, rhs: Self) -> Result<u64, TimestampError> {
        self.0
            .checked_sub(rhs.0)
            .ok_or(TimestampError::Underflow { lhs: self.0, rhs: rhs.0 })
    }

    /// Diferença em nanos saturada (zero se underflow).
    pub fn saturating_sub(self, rhs: Self) -> u64 {
        self.0.saturating_sub(rhs.0)
    }
}

impl fmt::Display for UnixNanos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secs = self.0 / 1_000_000_000;
        let nanos = self.0 % 1_000_000_000;
        write!(f, "{secs}.{nanos:09}")
    }
}

/// Timestamp monotônico em nanosegundos (não comparável com `UnixNanos`).
///
/// Imune a NTP jumps. Usado para medições internas de latência. Não tem
/// significado absoluto: só faz sentido como diferença entre dois pontos
/// medidos no mesmo processo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonotonicNs(pub u64);

impl MonotonicNs {
    /// Constrói a partir de inteiro cru.
    pub const fn from_nanos(v: u64) -> Self {
        Self(v)
    }

    /// Inteiro cru.
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Diferença em nanos para outro `MonotonicNs` (rhs).
    ///
    /// Erro se `self < rhs` — o que indicaria bug grave (clock monotônico
    /// não pode retroceder).
    pub fn checked_sub(self, rhs: Self) -> Result<u64, TimestampError> {
        self.0
            .checked_sub(rhs.0)
            .ok_or(TimestampError::Underflow { lhs: self.0, rhs: rhs.0 })
    }
}

impl fmt::Display for MonotonicNs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "monotonic+{}ns", self.0)
    }
}

macro_rules! ts_wrapper {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub UnixNanos);

        impl $name {
            /// Constrói a partir de `UnixNanos`.
            pub const fn new(ts: UnixNanos) -> Self {
                Self(ts)
            }

            /// Constrói a partir de nanos cru.
            pub const fn from_nanos(v: u64) -> Self {
                Self(UnixNanos::from_nanos(v))
            }

            /// Inteiro cru de nanos.
            pub const fn as_nanos(self) -> u64 {
                self.0.as_nanos()
            }

            /// Acesso ao `UnixNanos` interno.
            pub const fn unix(self) -> UnixNanos {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

ts_wrapper!(
    ExchangeTs,
    "Timestamp do evento conforme reportado pela venue. Sujeito a clock skew."
);
ts_wrapper!(
    RecvTs,
    "Timestamp de quando nosso socket recebeu o frame. Ground truth comum."
);
ts_wrapper!(
    ProcessTs,
    "Timestamp de quando o evento saiu do normalizer. Mede latência interna."
);

/// Computa duração entre dois `UnixNanos` em nanos saturado.
pub fn duration_ns(later: UnixNanos, earlier: UnixNanos) -> u64 {
    later.saturating_sub(earlier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_nanos_construction() {
        let a = UnixNanos::from_millis(1_000);
        assert_eq!(a.as_nanos(), 1_000_000_000);
        let b = UnixNanos::from_micros(2_000_000);
        assert_eq!(b.as_nanos(), 2_000_000_000);
    }

    #[test]
    fn checked_sub_detects_underflow() {
        let a = UnixNanos(10);
        let b = UnixNanos(20);
        assert!(matches!(a.checked_sub(b), Err(TimestampError::Underflow { .. })));
    }

    #[test]
    fn saturating_sub_returns_zero_on_underflow() {
        let a = UnixNanos(10);
        let b = UnixNanos(20);
        assert_eq!(a.saturating_sub(b), 0);
    }

    #[test]
    fn display_format() {
        let t = UnixNanos::from_nanos(1_700_000_000_123_456_789);
        assert_eq!(t.to_string(), "1700000000.123456789");
    }

    #[test]
    fn ts_wrappers_are_distinct_types() {
        let e = ExchangeTs::from_nanos(100);
        let r = RecvTs::from_nanos(200);
        // Tentar fazer `e == r` quebraria no compilador.
        assert_eq!(e.as_nanos(), 100);
        assert_eq!(r.as_nanos(), 200);
    }

    #[test]
    fn duration_ns_saturates() {
        let a = UnixNanos::from_nanos(1000);
        let b = UnixNanos::from_nanos(900);
        assert_eq!(duration_ns(a, b), 100);
        assert_eq!(duration_ns(b, a), 0);
    }
}
