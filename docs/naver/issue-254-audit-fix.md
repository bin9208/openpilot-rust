# Issue #254: populated live navigation snapshot

Issue: <https://github.com/bin9208/openpilot-rust/issues/254>. Parent: #497.
Baseline: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
Worktree: `backlog-fixes-497/openpilot-rust`; branch: `codex/fix-497-stale-runtime-defects`.

## Failure and API contract

`live/snapshot.rs` read `mainText`, `distanceText` and `turnType` from cereal
NavInstruction, which defines none of those fields. Safe dynamic getters turned
all three into null even when the optional live service was populated.

Map `maneuverPrimaryText -> mainText`, `maneuverType -> turnType` and numeric
`maneuverDistance -> distanceM` in meters. The existing Drive Insights normalizer
already consumes `distanceM` and `turnType`; no browser production change is
needed. Keep `distanceText` as a metric display string: nearest meter below 1 km,
one decimal km below 10 km and whole km thereafter, with compact `m`/`km` suffixes
matching existing navigation display conventions. Numeric meters retain their
original precision and do not depend on `IsMetric`. An absent distance field,
negative distance or nonfinite value produces null for both distance fields.
Present default cereal fields remain empty text/zero meters, distinct from an
absent field. Optional service liveness/absence and route broker behavior are
unchanged; no navigation producer or vehicle-control logic changes.

Tests were written before implementation in `snapshot_tests.rs`. Actual populated
NavInstruction cereal events are serialized, decoded and passed through the
production snapshot path. Cases cover Unicode text, schema defaults, absent
fields, invalid distances, JSON encoding and m/km formatting boundaries.

The existing actual-msgq/HTTP probes now assert the old source response is exactly
the original three null navigation fields, assert the exact native correction,
then compare every other response field using the existing normalization.
Nothing outside `services.navInstructionCarrot` is exempted. App wiring uses the
same assertion with its explicit default-field expectations. The populated probe
passes its actual native HTTP response to `carrot_server_navigation_ui.mjs`,
which executes the real browser normalizer and checks 35.5 meters and `turn`.
All raw/compact WebSocket, camera, status, permission, cache and cleanup checks
remain in place.

## Hosted verification handoff

Run only in GitHub-hosted Actions, from repository root, with the parent's
existing original-source/full-cereal dependencies, `ORIGINAL_PARAMS_BINDING` and
Node.js >=20 setup:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --lib live::snapshot::snapshot_tests
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --example carrot_live
python3 -P rust/tools/carrot_server_live.py --binary rust/target/debug/examples/carrot_live --output "$RUNNER_TEMP/issue254-live"
python3 -P rust/tools/carrot_server_live_app.py --binary rust/target/debug/examples/carrot_live --output "$RUNNER_TEMP/issue254-app"
```

The first probe executes the browser assertion automatically and retains its
stdout/exit in `family/navigation-browser.json`. Adjust binary location if
Actions sets `CARGO_TARGET_DIR`. For baseline RED, in a separate hosted checkout
with the candidate test files, restore only original production snapshot and
keep test registration:

```sh
python3 - <<'PY'
from pathlib import Path
import subprocess
path = 'rust/crates/carrot-server/src/live/snapshot.rs'
source = subprocess.check_output(['git', 'show', 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:' + path], text=True)
Path(path).write_text(source + '\n#[cfg(test)]\n#[path = "snapshot_tests.rs"]\nmod snapshot_tests;\n')
PY
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --lib populated_cereal_navigation_maps_to_browser_fields_in_meters
```

Expected RED is the original null navigation object versus the populated expected
object, not a compile error. Exact-SHA hosted RED/GREEN, HTTP/browser verification
and aggregate parent acceptance remain pending. Local checks cover formatting,
syntax and scoped diff review only; no test/build/fixture execution occurred.
No commit, push, device or external-service mutation performed.
On 2026-10-23, owned Rust files passed `rustfmt --edition 2021 --check`, the browser
probe passed `node --check`, changed Python probes passed `ast.parse`, and the
owned patch passed `git diff --check`. These are syntax/format checks only.

Docs-Not-Needed: existing optional diagnostic API mapping correction adds no
setting; paired user guides are parent-owned and unchanged.
