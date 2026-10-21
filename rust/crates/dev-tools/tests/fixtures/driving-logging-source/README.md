`original-pending-egpu.json` contains only the twelve source callsite records
reported by the unchanged Python `model_logging_reference.py` source comparator
in the accepted full model pipeline artifact. It contains no Python source or
candidate-generated expected model output. The Rust collector pins these bytes
and the corrected original driving observation pins the complete `modeld.py`
source hash independently.

Provenance: `.analysis/scratch/2026-10-21-ci-publication/restored-model-pipelines-green.zip`,
ZIP SHA-256 `1dbaee82e962a7eb2ca78832207e48823bbb57133be9e60927c6cd618745d014`,
member `driving-daemon/1344-dual/report.json`, field
`logging.source_calls.pending_egpu_lines`. This source callsite inventory is
identical in all six recorded scenarios (canonical JSON SHA-256
`49ffe81478838153c9c3e2ead1b8261338a79555c546b4f8a19a58666801859e`).
Live collector output still determines all process IDs, frame records,
runtime timing aggregates and source log messages.
