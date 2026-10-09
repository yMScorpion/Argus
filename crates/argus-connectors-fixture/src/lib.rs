//! Connector determinístico para fixtures e geração sintética.
//!
//! Permite o pipeline rodar end-to-end sem depender de exchange real. Cada
//! geração é seedada, garantindo reprodutibilidade bit-a-bit em replay
//! tests.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod generator;
mod connector;

pub use connector::{FixtureCapabilityBuilder, FixtureConnector};
pub use generator::{
    FixtureGenerator, FixtureScenario, GeneratorConfig, RandomWalkParams,
};
