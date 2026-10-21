# Native driver-daemon comparison candidate (#296)

The optional `driver-daemon-native` feature registers a Rust controller and its
reference/mutation/live tests. It replays the original four1344/1928 × raw-on/off
cases using independently captured Python-produced packets: four warmups and ten
frame publications. Candidate message files cannot populate the oracle. The
original checker, collector and reference Python helpers remain present; no source
retirement or native compiler conversion is claimed by this candidate.

The first independent review found missing catalog-to-PKL linkage, unverified
loaded Python dependencies and lost VisionIPC socket cleanup. The parent found
that checking a canonical descriptor alone could miss an alternate index-selected
bundle. The candidate now follows the actual index entry for each camera and
checks model/metadata/warp hashes, rechecks all imported source hashes, and cleans
owned sockets/queues on success and failure while preserving foreign nodes.
Regressions include the orphaned-canonical/wrong-active-descriptor case, source and
packet mutations, real sockets and interrupted polls. Shared collector checks keep
driver-specific warnings, no errors/timing, queue/disk records and source-call
order; existing modeld checks remain intact.

Independent delta review found no remaining blocker. Only syntax, formatting,
source/data and diff inspection ran locally. The focused hosted gate must build
and strictly lint the new feature first, preserve the unchanged original checker
assertions in a fresh same-PKL capture, then run all library and live tests with
exact manifests, inputs and executable seals. The bounded result below passes. The
historical full model-pipeline archive contains native message output and cannot
serve as independent expected packets for those ten frames. No device result or
whole-family Python absence is inferred.

Accepted collector-enabled packet proof
[39630234405/job119326215013](https://github.com/bin9208/openpilot-rust/actions/runs/39630234405/job/119326215013)
checks sourcee4b30b8d197722eb31ce993c975dd631729aa937, workflow
df0e63c97878f410aae778facf33dde67bfac094. Parent and worker independently
audited artifact12359716524 (72,641,517bytes/232members, all CRCs), ZIP SHA256
`c006020804bc41ce7dcfe5317bd04b3841eee7d041205a8bb94a2eb33cfda742`.
Original manifest SHA256 is
`ee1806c15658f925cca31a1dd042d67e4a865f1a336ca8dff38b83cdcb3e4855`.
Parent checks all43 original file lengths/hashes, all15 explicit source inputs
and117 loaded repository Python sources against that exact Git commit. Four
original PKLs and real imported msgq extension identities are bound by hosted
before/after seals. Four native cases retain14 independent source expected
packets and10 frame inputs, original static reports and exact raw predictions.
Each collector exits0 with five log/disk records, no error/timing records and
the original source-call inventory. Reference/mutation/live tests and strict
Clippy pass. This is the library packet proof; the actual native CLI, optional
collector mode, positive raw-path and source-retirement gates remain separate.

The proposed `check-driver-daemon` binary is a separate evidence-driven CLI, not
a drop-in replacement for `check_driver_daemon.py`. It accepts the original
`--binary`, `--catalog`, `--models`, `--output`, and `--collector` spellings.
Additional requirements use `--native-*` names so original unique abbreviations
such as `--o` and `--m` retain their meaning. The native controller requires a
sealed original capture and manifest digest, the source commit, an existing
peer ELF, and an explicit checkout root. A separate original-only four-case
capture records the Python checker's optional no-collector path with
`logging: null`; Rust admission rejects mode mismatches before output creation.
That lane still needs a hosted original/native comparison. The Python checker
also compiles its peer under the output directory, which is not yet a native
CLI parity claim. Its original help/error
observations are captured independently in the optional hosted CLI extension.
The maintained Python checker stays until the remaining no-collector, CLI,
source-absence, and caller proofs complete. The parser retains raw Unix
argument bytes. The pending positive hosted gate passes a `0xff` parent alias
that resolves to an owned ASCII directory. It records full argv bytes, alias
identity and target before and after traffic, complete reports and streams,
then removes only the owned alias. Direct `0xff` output-directory components
remain outside the original checker's peer protocol: it calls UTF-8 `.encode()`
on the full path. Native admission rejects such paths too. Neither raw lane
has passed a hosted gate yet.
