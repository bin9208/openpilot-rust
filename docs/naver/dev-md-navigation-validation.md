# dev-md navigation control ownership

Issue #338, source e8e152dbcfd9574a1791ed12026d1880e87689b3.

Ported final carrot_navi.py control-owner semantics to the native WebSocket daemon.
Successful negotiation claims one live connection; same owner may renegotiate.
A second phone receives recoverable receiver_busy and1013close without mutating
the first session. Control events require successful negotiation. Disconnect
releases only that connection's ownership; a dropped task expires its weak token.
Ownership and negotiation are serialized so concurrent upgrades cannot both claim.
No host/device address or beacon policy changed.

Genuine RED against prior native binary: an unnegotiated event reached receiver
validation and returned unsupportedprotocol instead of negotiation-required.
GREEN actual source daemon and native daemon:two repeated secondphone rejections,
first sessionunchanged after each disconnect, sameowner renegotiation, final
replacementowner after disconnect. Own localhost listener, temporary Params,
no beacon/Cereal/device. Detailed evidence navi-owner-{red,source,green} under
2026-10-15-dev-md-resume scratch. Native full existing regression+ARM gate pending.
