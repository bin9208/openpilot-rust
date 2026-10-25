# LPA Cookie header allocation candidate (#439)

The existing request path matches cookies, filters secure cookies and performs
stable path-length sorting before formatting `name=value` pairs. The candidate
changes only that final formatting: a private borrowed-pair helper appends to
one String instead of building a String per pair and an intermediate Vec.
Names, values, separators, matching, ordering, redirects, methods and TLS stay
at the existing code paths.

The standalone Rust probe includes the actual production helper and freezes
the old formatting expression. It checks empty/duplicate/Unicode/delimiter
cases and exact8/1024-cookie headers. After warmup, thread-local allocator
tracking counts calls and requested bytes for64headers; the gate requires both
totals to fall. This is a bounded algorithm comparison, not an old full-LPA
build, independent cookie-selection oracle, peak RAM or device CPU measurement.
The existing complete original LPA serial/TLS/profile/notification lifecycle
check and affected strictClippy are still mandatory. Sources, executable and
measurements are retained in cloud evidence. No numerical improvement is
claimed until fresh Actions results are inspected. Local checks are formatting,
syntax and diff review only. No new public interface or dependency is added.


## Actual cloud result (2026-10-21)

Source `f903d9765c2f13e8b5ed24a0c4fad7256bb115ac` passed
[run39526488617/job118982236423](https://github.com/bin9208/openpilot-rust/actions/runs/39526488617/job/118982236423).
Diagnostic workflow head is separately `fef5bf9c56b6a276e25d92bdadee91380e20db16`.
Artifact12313257933 is1,147,609bytes; archive SHA256
`60fb779661c0c448c3da49dd9158b4f069ef2742edbc8d44514bdcb01a600f33`.
Actual production build and affected all-targets strictClippy passed.

For64headers of8cookies, allocation calls fell1,664→384 and requested bytes
68,480→44,352. For64headers of1,024cookies, calls fell196,736→832 and requested
bytes8,781,696→5,766,464. Header lengths were262/33,790bytes, with exact byte
parity and five explicit edge cases. These are host algorithm allocator totals;
no whole-process memory, elapsed-time, CPU or device improvement is claimed.

The complete original LPA comparison passed nine scenarios. Parent artifact
inspection confirmed all six original/native scenario structures are equal,
four hostname/root rejection attempts reached no server request, and native
lock/channel cleanup passed. Original/native configured2s HTTPS deadlines were
2.153608493s/2.014205422s, within the unchanged1.8–3.0s check. Source/binary
seals are retained. The companion source-policy job failed only because its
frozen estimator environment rejected the separately required #438 debug0
settings.04d29123 adds an exact two-key exception and rejection mutations,
keeping every other frozen field/command; its fresh policy job is pending.
Latest complete PR checks and integration are still required.
