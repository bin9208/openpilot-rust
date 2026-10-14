These two tiny CPU manifest fixtures were generated on 2026-10-14 from the
repository's original `rust/tools/model_export/schema.py` dataclasses using
`dataclasses.asdict` and `json.dumps(indent=2) + '\n'`. They cover version 1 with
omitted entrypoints and version 2 with an empty prepare range followed by policy.
They contain synthetic library/weight identities and do not represent executable
model binaries. Native tests compare exact JSON bytes, preserving field order,
indentation, null fields and negative scalar values. This is serialization evidence,
not numerical/model compiler equivalence.

`catalog-index.json` and `catalog-current.json` were generated with the original
`model_export.publish.publish` transaction, replacing only its numerical emitter
with a deterministic test emitter writing `graph` and `pipeline` bytes. They test
publication serialization/identity, not model lowering or inference. Source SHA
provenance uses the synthetic value `12` repeated 32 times.
