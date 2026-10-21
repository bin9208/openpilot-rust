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
exact manifests, inputs and executable seals. Its results remain pending. The
historical full model-pipeline archive contains native message output and cannot
serve as independent expected packets for those ten frames. No device result or
whole-family Python absence is inferred.
