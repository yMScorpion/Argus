@0xa1d1d1d1d1d1d1d1;

# ARGUS Canonical Market Event — schema declarativo.
#
# Status: declaração de intenção. Tipos Rust equivalentes em
# `crates/argus-schema/`. Geração via `capnpc` em fase 7+.

using Rust = import "/capnp/rust.capnp";

struct CanonicalEvent {
    schemaVersion @0 :UInt16;
    eventId @1 :UInt64;
    instrumentId @2 :Data;       # 16 bytes u128 little-endian
    venue @3 :Venue;
    sequenceId @4 :UInt64;
    exchangeTsNs @5 :UInt64;
    recvTsNs @6 :UInt64;
    processTsNs @7 :UInt64;
    venueStatus @8 :VenueStatus;
    quality @9 :DataQuality;

    payload :union {
        trade @10 :TradePayload;
        bookSnapshot @11 :BookSnapshot;
        bookDelta @12 :BookDelta;
        funding @13 :FundingPayload;
        markPrice @14 :MarkPricePayload;
        indexPrice @15 :IndexPricePayload;
        openInterest @16 :OpenInterestPayload;
        liquidation @17 :LiquidationPayload;
    }
}

enum Venue {
    fixture @0;
    binanceSpot @1;
    binanceUsdtFutures @2;
    binanceCoinFutures @3;
    bybitLinear @4;
    bybitInverse @5;
    okxPerp @6;
    okxSpot @7;
    coinbaseAdvanced @8;
    krakenPro @9;
    krakenFutures @10;
    bitgetFutures @11;
    deribit @12;
    hyperliquidPerp @13;
    dydxV4 @14;
    gmxV2 @15;
    vertexEdge @16;
    driftV2 @17;
}

enum VenueStatus {
    normal @0;
    degraded @1;
    maintenance @2;
    halted @3;
    unknown @4;
}

struct DataQuality {
    confidence @0 :UInt8;             # 0..=100
    sourceLatencyUs @1 :UInt32;
    flags @2 :List(QualityFlag);
}

enum QualityFlag {
    sequenceGap @0;
    outOfOrder @1;
    stale @2;
    initialization @3;
    resync @4;
    postReconnect @5;
    inferredAggressor @6;
    clockSkewDetected @7;
    lateEvent @8;
    storageLagged @9;
    degraded @10;
}

# Payloads — i128 / Decimal são serializados como Data + scale separado.
struct PriceTicks {
    ticks @0 :Data;              # 16 bytes i128 little-endian
    tickSizeMantissa @1 :Data;   # rust_decimal mantissa bytes
    tickSizeScale @2 :UInt32;
}

struct QtyLots {
    lots @0 :Data;
    lotSizeMantissa @1 :Data;
    lotSizeScale @2 :UInt32;
}

struct MoneyMinor {
    minor @0 :Data;              # 16 bytes i128
    decimals @1 :UInt8;
}

enum Side {
    buy @0;
    sell @1;
}

struct TradePayload {
    price @0 :PriceTicks;
    qty @1 :QtyLots;
    aggressor @2 :Side;
    tradeId @3 :Text;
    aggressorIsMaker @4 :Bool;
}

struct BookLevel {
    price @0 :PriceTicks;
    qty @1 :QtyLots;
}

struct BookSnapshot {
    snapshotId @0 :UInt64;
    bids @1 :List(BookLevel);
    asks @2 :List(BookLevel);
}

struct BookDelta {
    fromSeq @0 :UInt64;
    toSeq @1 :UInt64;
    bidChanges @2 :List(BookLevel);
    askChanges @3 :List(BookLevel);
}

struct FundingPayload {
    rateBps @0 :Int32;
    nextFundingTsNs @1 :UInt64;
    fundingIntervalSec @2 :UInt32;
}

struct MarkPricePayload {
    mark @0 :PriceTicks;
}

struct IndexPricePayload {
    index @0 :PriceTicks;
}

struct OpenInterestPayload {
    oiQty @0 :QtyLots;
    oiNotional @1 :MoneyMinor;
}

struct LiquidationPayload {
    side @0 :Side;
    price @1 :PriceTicks;
    qty @2 :QtyLots;
}
