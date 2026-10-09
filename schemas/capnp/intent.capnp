@0xb1b1b1b1b1b1b1b1;

# ARGUS Intent — payload tipado entre Terminal/Research/Plugin e Risk Daemon.
# Risk Daemon valida contra envelope antes de virar ordem.

struct Intent {
    schemaVersion @0 :UInt16;
    intentId @1 :Data;             # 16 bytes u128
    sourceProcess @2 :SourceRole;
    sourceId @3 :Text;             # hash assinado da strategy/plugin
    timestampNs @4 :UInt64;
    venue @5 :UInt16;              # corresponde a enum Venue em market_event.capnp
    instrumentId @6 :Data;
    side @7 :Side;
    kind @8 :IntentKind;
    quantityLots @9 :Data;
    priceTicks @10 :Data;          # opcional para market
    stopPriceTicks @11 :Data;      # opcional
    tpLevels @12 :List(TpLevel);
    timeInForce @13 :TimeInForce;
    reduceOnly @14 :Bool;
    postOnly @15 :Bool;
    leverageHintBps @16 :UInt32;   # 100bps = 1x; opcional
    rationale @17 :Text;
    riskTagsHint @18 :List(Text);
}

enum SourceRole {
    terminal @0;
    research @1;
    plugin @2;
}

enum Side {
    buy @0;
    sell @1;
}

enum IntentKind {
    openMarket @0;
    openLimit @1;
    closeMarket @2;
    closeLimit @3;
    adjustStop @4;
    adjustTp @5;
    cancelAll @6;
}

enum TimeInForce {
    gtc @0;       # Good Till Cancel
    ioc @1;       # Immediate Or Cancel
    fok @2;       # Fill Or Kill
    gtd @3;       # Good Till Date
}

struct TpLevel {
    fractionBps @0 :UInt16;        # parte da posição (0..=10000)
    priceTicks @1 :Data;
}
