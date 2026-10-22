# Issue #252: numbered Dashcam driver warnings

Issue: <https://github.com/bin9208/openpilot-rust/issues/252>. Parent: #497.
Baseline: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
Worktree: `backlog-fixes-497/openpilot-rust`; branch: `codex/fix-497-stale-runtime-defects`.

## Failure and scoped correction

The full schema names all six warning levels `driverDistracted1/2/3` and
`driverUnresponsive1/2/3`. Native report aggregation only recognized
`tooDistracted` in the driver category, preserving an inherited source defect.
Include all six numbered names in the same existing category. Category rising
edges, duplicate suppression, cross-segment state, FCW/stock FCW and LDW remain
unchanged. Warning generation and driver-monitoring policy are unaffected.

Tests were written before implementation in `warning_tests.rs`; they serialize
actual cereal Event messages and invoke production `State::segment`, rather than
searching source text. Each numbered name must count on its own; repeats,
duplicates and transitions between levels must not count additional episodes;
empty events permit a new episode. Existing categories have independent tests.

The retained original `state-carry` oracle has a continuous
`tooDistracted -> driverDistracted1 -> tooDistracted` sequence. The source counted
two episodes because the middle message incorrectly cleared the category.
Native now counts one. The comparison tool asserts the original count is exactly
two, substitutes exactly this one expected field with one, and compares every
other output as before. Source captures, hashes, limits and comparisons remain.

## Hosted verification handoff

All commands below are for GitHub-hosted Actions only, from repository root.
Parent owns aggregate workflow/dependency setup and exact-SHA run links.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --lib dashcam::report::aggregate::warning_tests
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --example dashcam_report
python3 -P rust/tools/carrot_server_dashcam_report_source_controls.py --output "$RUNNER_TEMP/issue252-source"
python3 -P rust/tools/carrot_server_dashcam_report_state_controls.py --output "$RUNNER_TEMP/issue252-state"
python3 -P rust/tools/carrot_server_dashcam_report_open_stream.py --output "$RUNNER_TEMP/issue252-open-stream"
python3 -P rust/tools/carrot_server_dashcam_report.py --binary rust/target/debug/examples/dashcam_report --source-captures "$RUNNER_TEMP/issue252-source" --state-captures "$RUNNER_TEMP/issue252-state" --open-stream-captures "$RUNNER_TEMP/issue252-open-stream" --output "$RUNNER_TEMP/issue252-compare"
```

The source controls require the existing full-cereal, zstandard and original
report dependencies. Adjust binary location if Actions sets `CARGO_TARGET_DIR`.

To obtain actual baseline RED without changing the tests, use a separate hosted
baseline checkout containing candidate test files and replace only production
aggregation as follows. This is a cloud recipe, not a locally executed step:

```sh
python3 - <<'PY'
from pathlib import Path
import subprocess
path = 'rust/crates/carrot-server/src/dashcam/report/aggregate.rs'
source = subprocess.check_output(['git', 'show', 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:' + path], text=True)
Path(path).write_text(source + '\n#[cfg(test)]\n#[path = "warning_tests.rs"]\nmod warning_tests;\n')
PY
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --lib numbered_driver_warnings_each_start_a_report_episode
```

Expected RED is an assertion count of zero instead of one, not a compile error.
Candidate GREEN, source-oracle comparison and full parent aggregate acceptance
are pending. Local work is limited to formatting, syntax and scoped diff checks;
no functional test/build/fixture execution occurred. No commit or push performed.
On 2026-10-23, owned Rust files passed `rustfmt --edition 2021 --check` and the
owned patch passed `git diff --check`; the report comparison tool passed Python
`ast.parse` syntax validation. Concurrent workers' files were not modified.

Docs-Not-Needed: report warning recognition correction adds no setting or guide
workflow; paired user guides are parent-owned and unchanged.
