//! Trait `VenueConnector` e abstrações comuns para conectores.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

use argus_capability::{ChannelKind, VenueCapability};
use argus_core_types::{InstrumentId, Venue};
use argus_errors::Result;
use argus_schema::CanonicalEvent;

/// Especifica subscription a um channel de uma venue/instrument.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketSubscription {
    /// Venue.
    pub venue: Venue,
    /// Instrument.
    pub instrument: InstrumentId,
    /// Channel kind.
    pub channel: ChannelKind,
    /// Cadência preferida em ms (None = default da venue).
    pub cadence_ms: Option<u16>,
}

/// Snapshot REST de uma venue (raw bytes para o connector decidir como
/// parsear).
#[derive(Debug, Clone)]
pub struct RawSnapshot {
    /// Bytes do payload.
    pub bytes: Vec<u8>,
    /// Endpoint de origem (para audit).
    pub endpoint: String,
    /// Status HTTP retornado.
    pub status: u16,
}

/// Trait que todo connector implementa.
///
/// Versão atual (Fase 3) é sync para simplicidade. Fase 8+ migra para async
/// quando conectarmos venues reais com tokio runtime.
pub trait VenueConnector: Send + Sync {
    /// Venue suportada por este connector.
    fn venue_id(&self) -> Venue;

    /// Descritor de capabilities.
    fn capabilities(&self) -> &VenueCapability;

    /// Inicia uma subscription. Retorna iterator de eventos canônicos.
    ///
    /// A implementação concreta decide encoding interno (channel,
    /// callback, future). Para a Fase 3 usamos iterator drainable
    /// retornado pelo fixture connector.
    fn subscribe(
        &self,
        sub: MarketSubscription,
    ) -> Result<Box<dyn Iterator<Item = CanonicalEvent> + Send>>;

    /// Snapshot REST sob demanda (para resync após gap).
    fn fetch_snapshot(&self, instrument: InstrumentId) -> Result<RawSnapshot>;
}
