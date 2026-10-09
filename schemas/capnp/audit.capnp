@0xc1c1c1c1c1c1c1c1;

# ARGUS Audit Log entry — append-only, hash-chained.

struct AuditEntry {
    seq @0 :UInt64;                # monotônico por instalação
    timestampNs @1 :UInt64;
    eventType @2 :AuditEventType;
    payload @3 :Data;              # JSON serialized struct específico do evento
    prevHash @4 :Data;             # 32 bytes blake3
    thisHash @5 :Data;             # 32 bytes blake3
}

enum AuditEventType {
    daemonBoot @0;
    daemonShutdown @1;
    configLoaded @2;
    envelopeChanged @3;
    intentReceived @4;
    intentRejected @5;
    intentAccepted @6;
    orderPlanned @7;
    orderSubmitted @8;
    venueAck @9;
    fill @10;
    cancel @11;
    reconciliationDiff @12;
    killSwitchActivated @13;
    killSwitchDeactivated @14;
    stateTransition @15;
    securityEvent @16;
    modeChanged @17;
}
