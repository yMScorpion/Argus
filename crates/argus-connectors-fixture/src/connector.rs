use smallvec::SmallVec;

use argus_capability::{
    BookDepth, ChannelKind, OrderTypeCap, SequenceModel, VenueCapability, VenueChannel,
};
use argus_connectors_core::{MarketSubscription, RawSnapshot, VenueConnector};
use argus_core_types::{InstrumentId, Venue};
use argus_errors::{ArgusError, Result};
use argus_schema::CanonicalEvent;

use crate::generator::{FixtureGenerator, FixtureScenario, GeneratorConfig};

/// Builder de `VenueCapability` para a venue fixture.
#[derive(Debug)]
pub struct FixtureCapabilityBuilder;

impl FixtureCapabilityBuilder {
    /// Cria descritor padrão (L2 diff + trades).
    pub fn build() -> VenueCapability {
        let mut channels: SmallVec<[VenueChannel; 16]> = SmallVec::new();
        channels.push(VenueChannel {
            kind: ChannelKind::Depth,
            depth: Some(BookDepth::L2Diff),
            cadence_ms: SmallVec::new(),
            sequence_model: SequenceModel::SeqMonotonic,
            latency_p50_ms_typical: 0,
        });
        channels.push(VenueChannel {
            kind: ChannelKind::Trades,
            depth: None,
            cadence_ms: SmallVec::new(),
            sequence_model: SequenceModel::SeqMonotonic,
            latency_p50_ms_typical: 0,
        });
        let mut order_types: SmallVec<[OrderTypeCap; 16]> = SmallVec::new();
        for ot in [
            OrderTypeCap::Limit,
            OrderTypeCap::Market,
            OrderTypeCap::PostOnly,
            OrderTypeCap::Ioc,
            OrderTypeCap::Fok,
        ] {
            order_types.push(ot);
        }
        VenueCapability {
            venue: Venue::Fixture,
            version: "0.1.0".into(),
            channels,
            order_types,
            supports_reduce_only: true,
            supports_hedge_mode: false,
            supports_server_oco: false,
            has_testnet: false,
            rest_rate_limit_per_sec: u32::MAX,
            typical_latency_p50_ms: 0,
            supports_atomic_replace: true,
            has_l3: false,
        }
    }
}

/// Connector que rouba fixture/syntactic generator e o veste como
/// `VenueConnector`.
#[derive(Debug)]
pub struct FixtureConnector {
    capability: VenueCapability,
    scenario: FixtureScenario,
    config: GeneratorConfig,
}

impl FixtureConnector {
    /// Constrói.
    pub fn new(scenario: FixtureScenario, config: GeneratorConfig) -> Self {
        Self {
            capability: FixtureCapabilityBuilder::build(),
            scenario,
            config,
        }
    }
}

impl VenueConnector for FixtureConnector {
    fn venue_id(&self) -> Venue {
        Venue::Fixture
    }

    fn capabilities(&self) -> &VenueCapability {
        &self.capability
    }

    fn subscribe(
        &self,
        _sub: MarketSubscription,
    ) -> Result<Box<dyn Iterator<Item = CanonicalEvent> + Send>> {
        let gen = FixtureGenerator::new(self.scenario.clone(), self.config);
        Ok(Box::new(gen))
    }

    fn fetch_snapshot(&self, _instrument: InstrumentId) -> Result<RawSnapshot> {
        // Fixture não tem REST; snapshot vem pelo stream.
        Err(ArgusError::Venue {
            venue: "fixture".into(),
            message: "fixture connector does not support REST snapshots".into(),
        })
    }
}
