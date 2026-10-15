# dev-md selected VW reference refresh (#338)

This is a **mixed reference snapshot**, not a claim that the retained Python tree or the Rust runtime matches all of latest upstream. Repository common baseline: `43203371004e035bdb70a00a8dad29a4b657c6c3`; current source audit tip: `e8e152dbcfd9574a1791ed12026d1880e87689b3`. Only the selected changes below are imported into existing CI comparison oracles. Runtime daemon selection and Rust implementation are unchanged by this reference-only change; no Python runtime path is added.

## Exact imported source

* `eb58e6d7b04ef3dbfc31a3b442bb17616dd51edd`: remove the VW-specific learned-ratio bypass from `resolve_vehicle_model_steer_ratio`, remove the fourth argument at the controlsd call, and retain upstream's live-setting propagation tests. All platforms honor manual CustomSR and live scaling with existing invalid-value fallback. The helper is byte-identical to upstream blob `e679268b3e488a7eac11720a37f43d4a8fa2bb7a`; upstream tests are byte-identical to blob `cffe4c8faa9ddcb0c6ae691b7aff90fd5642a888`.
* `a315e806b03b86c2f97f720c137f6595fa046244`: only the MEB longitudinal Ki breakpoint/value changes to `[0.]` / `[0.]`. Retained VW interface is byte-identical to upstream blob `8dcb9b4127455b96a36655cc5a1d870638ddb64f`.
* The `controlsd.py` VehicleModel-update block is byte-identical to the corresponding `eb58e6d7` block. The **whole controlsd file is not** identical to that commit or latest tip: unrelated upstream changes are intentionally not imported before their Rust behavior is ported. No latest-source global pin is asserted.

SHA-256 of the resulting selected oracle files:

| File | SHA-256 |
| --- | --- |
| `openpilot/selfdrive/controls/lib/steer_ratio.py` | `87fc3c01b2c3cfdd0e9df0deacd5dc2416df6edf76b18536d7d2f014d1ce77ee` |
| `openpilot/selfdrive/controls/tests/test_steer_ratio.py` | `953a057a040c938551046ddd4bc5de36ba20984ce947a6455043cede908d85d0` |
| `openpilot/selfdrive/controls/controlsd.py` | `a9c47a3431a45049f962c90ecf040ef927ae6c88b8e37c7ae485efa90fa474d6` |
| `opendbc_repo/opendbc/car/volkswagen/interface.py` | `a9ba2b0bdc936dda531b72711adbd4e9a254d3808b44fe70a0447f07bd7ced8c` |

Licensing/provenance headers and unrelated source code are preserved. The source patch consists of helper/call/test hunks from `eb58e6d7` and two Ki lines from `a315e806`; it does not copy latest controlsd wholesale.

## Reference closures and manifest handling

`rust/tools/controlsd_source.py` reads/executes the retained Controls class and imports the actual steering-ratio helper, longitudinal/lateral controllers, vehicle model and source interface registry while replacing IPC/Params/clock boundaries for tests. The refreshed helper and call therefore stay in the same reference closure. No reference output is synthesized or substituted.

`rust/tools/card_qa/volkswagen/source.py` imports the actual Volkswagen `CarInterface` and calls `get_params` before tracing controller/state behavior. Thus the existing `check_card_volkswagen.py` comparisons will observe the changed Ki tuning; Rust snapshots/numerical expectations must agree rather than weakening comparisons.

`rust/tools/generate_control_registry.py` consumes all original interface classes but its generated registry stores interface/acceleration-family/feedforward metadata, not longitudinal Ki tuning. Regenerated to ignored scratch with the existing `.venv`, without dependency installation. The result is byte-for-byte unchanged: 279 entries, SHA-256 `5092661e55931389dd720e8b879dfa580c01dd863d346e7ab68ec1e0adbe0a0a`. Consequently only the Volkswagen interface hash in `rust/crates/control-policy/data/registry-provenance.json` is refreshed from `6c78f1c7f34ecf9db116531f16aadec8a0f17a6d1d435017406a893e0a34e3eb` to the hash above. No other provenance hashes or generated registry bytes are updated. Search of retained provenance/manifests found no separate pinned controlsd/helper hash to update. `rust/upstream/carrot-wip.json` is the source-delta inventory, not a declaration that all retained files equal its tip; it remains unchanged.

## Validation and remaining evidence

* System `python3 -m pytest` is unavailable; used the pre-existing `.venv/bin/python`, with no installs.
* RED after importing upstream test suite alone: 18 failures / 1 pass. Seventeen failures identify the old four-argument helper; the VW live-setting case demonstrates the old bypass returning 14.97 instead of manual 15.9.
* GREEN after applying exact helper/call hunks: `.venv/bin/python -m pytest -c /dev/null --noconftest -p no:cacheprovider openpilot/selfdrive/controls/tests/test_steer_ratio.py -q` passes all 19 cases. This includes real controlsd prefix execution through `VM.update_params`, manual/scaled/default/invalid inputs and both VW/non-VW settings.
* Existing `.venv/bin/python rust/tools/generate_control_registry.py --output .analysis/scratch/2026-10-15-dev-md-reference/registry.json` succeeds; generated registry equality checked before targeted provenance update.
* Exact helper/test/interface bytes and controlsd VehicleModel block compared with the pinned upstream Git objects; scoped `git diff --check` passes.

No Cargo build was requested or run for this source-only task. Full controlsd/card Volkswagen numerical differential checks against the Rust binaries remain the parent integration/CI gate and must be rerun using matching source fixtures. These selected oracle changes establish no full latest-source parity, vehicle validation or performance result.
