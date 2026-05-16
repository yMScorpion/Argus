use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::ids::InstrumentId;
use crate::venue::Venue;

/// Tipo de contrato negociado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContractType {
    /// Spot — entrega imediata da base asset contra quote asset.
    Spot,
    /// Perpetual swap linear (PnL em quote).
    LinearPerp,
    /// Perpetual swap inverso (PnL em base).
    InversePerp,
    /// Future com vencimento, settlement linear.
    LinearFuture,
    /// Future com vencimento, settlement inverso.
    InverseFuture,
    /// Opção européia ou americana.
    Option(OptionKind),
}

/// Kind da opção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OptionKind {
    /// Call.
    Call,
    /// Put.
    Put,
}

/// Settlement asset / margin asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Settlement {
    /// Settlement em USDT.
    Usdt,
    /// Settlement em USDC.
    Usdc,
    /// Settlement em BUSD (legacy).
    Busd,
    /// Settlement em moeda base (inverse).
    Base,
    /// Settlement em USD fiat.
    Usd,
    /// Settlement em EUR fiat.
    Eur,
    /// Outro asset; nome explícito.
    Other,
}

/// Referência canônica a um instrumento (estável entre processos).
///
/// Esta estrutura captura tudo que distingue um instrumento operacionalmente:
/// não basta `BTCUSDT` — dois venues podem ter `BTCUSDT` com tick size,
/// lot size, contract type e settlement diferentes. `InstrumentRef` é o que
/// vai dentro de payloads cross-process; `InstrumentId` é o handle compacto.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentRef {
    /// Venue.
    pub venue: Venue,
    /// Símbolo humano (como a venue chama).
    pub venue_symbol: String,
    /// Asset base (ex: "BTC").
    pub base: String,
    /// Asset quote (ex: "USDT").
    pub quote: String,
    /// Tipo de contrato.
    pub contract: ContractType,
    /// Asset de settlement / margin.
    pub settlement: Settlement,
    /// Multiplicador do contrato (1 para spot/lin perp; varia para
    /// inverse/options).
    pub contract_multiplier: rust_decimal::Decimal,
    /// Vencimento em timestamp Unix nanos (None para perp/spot).
    pub expiry_ns: Option<u64>,
    /// Strike para opção (None se não-opção).
    pub strike: Option<rust_decimal::Decimal>,
    /// Aliases reconhecidos para o mesmo instrumento (BTC-USDT-PERP, etc).
    /// `SmallVec` evita alocação no caso comum de zero ou um alias.
    #[serde(default)]
    pub aliases: SmallVec<[String; 2]>,
}

impl InstrumentRef {
    /// Computa o `InstrumentId` determinístico via hash blake3 de campos
    /// canônicos.
    ///
    /// Dois `InstrumentRef`s representando o mesmo instrumento devem produzir
    /// o mesmo `InstrumentId`. Aliases não entram no hash (são apenas hint
    /// de nomenclatura).
    pub fn instrument_id(&self) -> InstrumentId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.venue.as_str().as_bytes());
        hasher.update(b"\x00");
        hasher.update(self.base.as_bytes());
        hasher.update(b"\x00");
        hasher.update(self.quote.as_bytes());
        hasher.update(b"\x00");
        hasher.update(format!("{:?}", self.contract).as_bytes());
        hasher.update(b"\x00");
        hasher.update(format!("{:?}", self.settlement).as_bytes());
        hasher.update(b"\x00");
        hasher.update(self.contract_multiplier.to_string().as_bytes());
        if let Some(exp) = self.expiry_ns {
            hasher.update(b"\x01");
            hasher.update(&exp.to_le_bytes());
        }
        if let Some(strike) = self.strike {
            hasher.update(b"\x02");
            hasher.update(strike.to_string().as_bytes());
        }
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        // Pega os 16 bytes mais significativos para u128.
        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&bytes[..16]);
        InstrumentId::from_u128(u128::from_le_bytes(id_bytes))
    }

    /// Indica se contract type usa funding rate (perp).
    pub fn has_funding(&self) -> bool {
        matches!(self.contract, ContractType::LinearPerp | ContractType::InversePerp)
    }

    /// Indica se contract type tem settlement linear (PnL em quote).
    pub fn is_linear(&self) -> bool {
        matches!(
            self.contract,
            ContractType::LinearPerp | ContractType::LinearFuture | ContractType::Spot
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn btc_usdt_perp() -> InstrumentRef {
        InstrumentRef {
            venue: Venue::BinanceUsdtFutures,
            venue_symbol: "BTCUSDT".into(),
            base: "BTC".into(),
            quote: "USDT".into(),
            contract: ContractType::LinearPerp,
            settlement: Settlement::Usdt,
            contract_multiplier: dec!(1),
            expiry_ns: None,
            strike: None,
            aliases: SmallVec::new(),
        }
    }

    #[test]
    fn instrument_id_is_deterministic() {
        let a = btc_usdt_perp();
        let b = btc_usdt_perp();
        assert_eq!(a.instrument_id(), b.instrument_id());
    }

    #[test]
    fn instrument_id_changes_with_venue() {
        let mut a = btc_usdt_perp();
        a.venue = Venue::BybitLinear;
        let b = btc_usdt_perp();
        assert_ne!(a.instrument_id(), b.instrument_id());
    }

    #[test]
    fn instrument_id_changes_with_contract_type() {
        let mut a = btc_usdt_perp();
        a.contract = ContractType::Spot;
        let b = btc_usdt_perp();
        assert_ne!(a.instrument_id(), b.instrument_id());
    }

    #[test]
    fn aliases_dont_affect_id() {
        let mut a = btc_usdt_perp();
        a.aliases.push("BTC-USDT-PERP".into());
        let b = btc_usdt_perp();
        assert_eq!(a.instrument_id(), b.instrument_id());
    }

    #[test]
    fn perp_has_funding() {
        let p = btc_usdt_perp();
        assert!(p.has_funding());
        assert!(p.is_linear());
    }
}
