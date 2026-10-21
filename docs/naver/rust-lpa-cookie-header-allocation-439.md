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
