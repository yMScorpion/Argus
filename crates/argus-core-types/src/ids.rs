//! IDs canônicos opaque.
//!
//! Cada tipo de ID é um newtype distinto para impedir mistura acidental no
//! compilador (ex: passar `OrderId` onde se espera `IntentId`). Implementações
//! de `Display`, `Debug`, `Hash` e `serde` são derivadas.
//!
//! Convenções:
//! - IDs gerados por nós internos: `u128` (UUIDv7-like, com componente
//!   monotônico para sort/index).
//! - IDs externos (vindos da venue): `String`.
//! - Sequence/snapshot/event/trade IDs internos: `u64` (suficientes; bump para
//!   `u128` se algum canal mostrar overflow projetado em uma vida humana).

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! u64_id {
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
            Default,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// Constrói a partir de inteiro cru. Use sparingly — prefira
            /// gerar via factory que garante unicidade.
            pub const fn from_raw(v: u64) -> Self {
                Self(v)
            }

            /// Inteiro cru.
            pub const fn raw(self) -> u64 {
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

macro_rules! u128_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        ///
        /// Wire format: string UUID. Internamente: `u128`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        pub struct $name(pub u128);

        impl $name {
            /// Constrói a partir de u128 cru.
            pub const fn from_u128(v: u128) -> Self {
                Self(v)
            }

            /// Inteiro cru.
            pub const fn raw(self) -> u128 {
                self.0
            }

            /// Gera um novo ID via UUIDv7 (componente time + random).
            pub fn new_v7() -> Self {
                let u = ::uuid::Uuid::now_v7();
                Self(u.as_u128())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let u = ::uuid::Uuid::from_u128(self.0);
                fmt::Display::fmt(&u, f)
            }
        }

        impl Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let u = ::uuid::Uuid::from_u128(self.0);
                s.collect_str(&u)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                let u = ::uuid::Uuid::parse_str(&s)
                    .map_err(|e| ::serde::de::Error::custom(format!("invalid uuid: {e}")))?;
                Ok(Self(u.as_u128()))
            }
        }
    };
}

macro_rules! string_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Constrói a partir de qualquer `Into<String>`.
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Slice do conteúdo.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }
    };
}

// IDs canônicos da suíte.
u64_id!(SequenceId, "Sequence id contínuo por (venue, symbol, channel).");
u64_id!(SnapshotId, "ID de um snapshot de orderbook (por venue/symbol).");
u64_id!(EventId, "ID interno de evento canônico (monotônico por canal).");

u128_id!(InstrumentId, "Handle compacto de instrumento (hash de InstrumentRef).");
u128_id!(IntentId, "ID de uma intenção emitida pelo Terminal/Research/Plugin.");
u128_id!(OrderId, "ID interno de ordem (não confundir com venue_order_id).");
u128_id!(SubscriptionId, "ID de uma assinatura (símbolo+canal+qos).");
u128_id!(StrategyId, "ID de uma strategy (binding ao hash do plugin assinado).");
u128_id!(PluginId, "ID de um plugin instalado.");
u128_id!(InstanceId, "ID da instância de processo (gerado no boot).");
u128_id!(TraceId, "ID de trace OpenTelemetry-compatível para correlação.");
u128_id!(CorrelationId, "ID de correlação multi-step (ex: intent → order → fill).");
u128_id!(ReplaySessionId, "ID de uma sessão de replay (incluindo branch).");

string_id!(SymbolId, "Símbolo humano (como aparece em UI). Não único cross-venue.");
string_id!(ChannelId, "Nome de canal de stream da venue (ex: \"depth100ms\").");
string_id!(ClientOrderId, "client_order_id idempotente enviado à venue.");
string_id!(TradeId, "trade_id como reportado pela venue.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_distinct_types() {
        // Compile-time check: passar OrderId onde se espera IntentId quebra.
        let _intent: IntentId = IntentId::new_v7();
        let _order: OrderId = OrderId::new_v7();
        // O test de mistura acidental falharia no compilador — apenas
        // asseguramos aqui que os tipos são deriváveis.
    }

    #[test]
    fn u64_id_serde_is_transparent() {
        let s = SequenceId::from_raw(42);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "42");
        let back: SequenceId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn u128_id_v7_monotonic_in_practice() {
        let a = IntentId::new_v7();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = IntentId::new_v7();
        assert!(b.raw() > a.raw(), "v7 ids should be monotonic in time");
    }

    #[test]
    fn string_id_construction() {
        let s: SymbolId = "BTCUSDT".into();
        assert_eq!(s.as_str(), "BTCUSDT");
        let s2 = SymbolId::new("ETHUSDT".to_string());
        assert_eq!(s2.as_str(), "ETHUSDT");
    }
}
