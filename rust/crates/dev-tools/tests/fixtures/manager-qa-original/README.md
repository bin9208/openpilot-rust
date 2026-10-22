# Independent manager QA observations

These ten files are only the original-Python side of the platform-runtime
comparison in run 39717847363, job 119624142986, checked-out source
`fc0f017bd96419c81af878ef211ed15152fc2eda`, workflow commit
`c6929deae721b30e7eea0478dea78fc94b31685e`, artifact 12399722817.
The ZIP SHA-256 is `41fcea8bc33b6cc454b3926228ece51989c0b5baaf647c5cbf7ada03055020d4`;
all 4,165 members passed CRC validation. `manifest.json` binds each copied raw
original file. No native result, executable Python source, or imported module
is stored here.

Seven `source/*.json` files preserve complete status, ordered trace, raw Params
byte arrays, and environment fields for the unchanged source controller.
`logging/source/result.json` preserves its full lifecycle/Params result, and
`logging/source-records.json` preserves all four original ZMQ packets including
dynamic host, process, thread, timestamp, and path fields.
`logging/synthetic-environment.json` is the original independent input
environment. Any comparison that treats dynamic fields must name each
normalization; retaining raw files does not turn those fields into constants.
The seven checked-in manager car-name arrays are separate source-derived inputs
and are not duplicated here.

This artifact lacks a complete imported-source roster and held source/binary
identity receipts. It can seed the candidate comparison but is not alone a
retirement proof. A fresh hosted capture must pin those dependencies and then
run the same native checker against checkout and source-absent roots before any
Python deletion.
