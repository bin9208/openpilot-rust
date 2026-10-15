# dev-md Volkswagen steering and integral-policy port — #338

Source target: `carrot-source/carrot-wip` at `e8e152dbcfd9574a1791ed12026d1880e87689b3`; source comparison ancestor `43203371004e035bdb70a00a8dad29a4b657c6c3`; Rust dev base `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Work is local on dev-md, with no commit, push or device action.

## Source behavior and changes

- `eb58e6d7b04ef3dbfc31a3b442bb17616dd51edd` removes the VW MEB bypass from `controls/lib/steer_ratio.py` and its controlsd call. Native `control-policy/src/drive.rs` now exposes the same three-argument policy; `controlsd/src/controller.rs` reads CustomSR and SteerRatioRate each control cycle and applies it for every platform. CustomSR is in tenths, overrides scaling only when finite and greater than 1 after conversion; rate must be finite and within 30..200, otherwise 100 is used. Learned-ratio minimum remains 0.1.
- `a315e806b03b86c2f97f720c137f6595fa046244` changes VW MEB CarParams Ki breakpoint/value arrays from `[0,30]/[0.4,0]` to `[0]/[0]`. Native `card/src/brands/volkswagen/parameters.rs` now does that for both MEB generations. Radar delay 0.8 s, longitudinal actuator delay 0.5 s and unrelated tuning remain unchanged. This is the source CarParams default policy; existing generic live tuning refresh remains as in source, so it is not a hard runtime prohibition on user LongTuningKiV.
- Requested provenance inspection confirms `c95bf4e6` is Tesla EAC error initialization and `5479d127` is Tesla DAS/navigation speed-priority integration; neither belongs in this bounded VW change.

## RED and GREEN evidence

2026-10-15, native host tests, coordinated target cache `/home/fma1/rust/cache/cargo-target`; `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, dev/test debug=0. Disk checked before each Cargo invocation: 366–367 GiB available, well above the required floor. Existing native SDK bin directory appended to PATH for assembler lookup; no dependency install.

RED before production edits:

- `cargo test --manifest-path rust/Cargo.toml -p openpilot-control-policy --test math`: expected failures for custom ratio on MEB (15 vs 20) and new live-setting/fallback vectors.
- `cargo test --manifest-path rust/Cargo.toml -p openpilot-card --test volkswagen_params`: expected failure for two Ki points vs required one. Initial environment error (`g++` cannot find `as`) was repaired using the existing SDK, then the behavioral failure was observed.

GREEN / broader verification:

- `control-policy --test math`: 6 passed after the policy edit, including final-source custom/live/fallback vectors.
- `controlsd` full package: 7 passed (2 unit, 5 state), doc-tests passed.
- `card --test volkswagen_params --test volkswagen_assets`: 5 passed; checks both MEB generations and existing required DBC/gear/HCA failure behavior.

- `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-control-policy -p openpilot-card -p openpilot-controlsd --all-targets -- -D warnings`: passed.
- rustfmt check on all touched Rust files and `git diff --check`: passed.

Full-package attempts were made rather than presenting focused checks as a green suite. `cargo test -p openpilot-control-policy -p openpilot-card -p openpilot-controlsd` stopped at card `hyundai_acc` tests `canfd_scc_bytes_and_recovery_when_pedals_holds_and_enable_variants` and `legacy_scc_bytes_when_camera_and_longitudinal_variants`: HYUNDAI_FIXTURE_DIR unset (`NotPresent`). A separate full control-policy/controlsd attempt passed math then stopped at control-policy `native_owned_buffers_dimensions_and_library_lifetime` and `nano_loads_validated_layers_once_and_reuses_them`: CONTROLS_NUMERICS unset. These fixture prerequisites are outstanding broader verification, not VW behavior failures.

No Python runtime or source artifact was added. Relevant Korean/English steering guide synchronization and user-doc validation remain with the parent source-integration task. No device or driving acceptance is inferred.
