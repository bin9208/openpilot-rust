# lagd masked correlation input allocation (#429)

Issue: https://github.com/bin9208/openpilot-rust/issues/429

Status (2026-10-21): source change and cloud probe prepared; no functional or
allocation result has been claimed. The user permits local syntax, formatting,
and diff review only. Run builds, tests, and measurements in GitHub Actions on
the exact candidate SHA, then record that SHA, Actions URL, measured rows, and
any failure here.

`Correlator::masked` formerly materialized masked `expected` and `actual`
`Vec<f64>` values before the FFT. It now clones stack-only iterators over the
borrowed values and mask. The actual FFT still precedes the reversed expected
FFT; both squared FFTs still square masked values. FFT buffers, convolution,
rounding, normalization, and downstream daemon policy are unchanged. The
source-level expected saving is two vector allocations and `2 * length * 8`
requested bytes per call. The cloud probe must measure this; source reasoning
alone is not a performance result.

The standalone probe at `rust/tools/lagd_masked_allocation_probe.rs` warms one 256-point
correlator before counting, then invokes production `masked` 16 times at input
lengths 64 and 128 with the same deterministic mixed masks in old and current
checkouts. A process-global Rust allocator counts calls and requested bytes;
the counter does not measure C++ `malloc`, peak memory, CPU, or vehicle
latency. It also prints a fingerprint of the complete numerical output. The
probe source is compiled unchanged against both libraries. `edb42bf0` and
pre-change `b1b62bb` have identical `lagd`, `pocketfft`, and Cargo lock sources.

The deep `estimation-runtime` Actions job now contains **Measure masked lagd
input allocation against unchanged source** after the lagd QA build. The step
fetches pinned `edb42bf095d6a3e3b38cc0a4546976dfb710363e` and builds
the old `openpilot-lagd` library in `$RUNNER_TEMP/lagd429-old-target`, isolated
from the current workspace target because native build caches contain absolute
source paths. Both builds use `CARGO_INCREMENTAL=0` and the job's existing
default dev profile, matching the earlier estimator build in that target.
The identical standalone source is linked once
against each library with the same `rustc` flags. Each binary writes two raw
`MASKED_ALLOC` records into `$RUNNER_TEMP/estimation/lagd/masked-allocation-429`.

The current probe's `--compare OLD_LOG CURRENT_LOG` mode is the gate. It
requires exactly two complete records in the 64, 128 order, 16 iterations in
each, identical output fingerprints, and measured old-minus-current deltas of
32 Rust allocator calls and `16 * 2 * length * 8` requested bytes. It writes
`comparison.log` on success; raw old/current logs and comparison output are
included in the existing `rust-estimation-evidence` artifact. The existing
numerical/source-loop/daemon checks remain required.

Disk guards precede the 8 GiB old build, 1 GiB current build, and both 128 MiB
probe compilations. They must preserve the repository's 25 GiB free-space
floor plus estimated growth. The probe adds a narrow measured allocation and
bitwise-output comparison only.
