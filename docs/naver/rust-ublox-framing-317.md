# u-blox split sync-prefix recovery (#317)

Applied over `26ca85aad602bfe0e9661567e2bcfd69e7bc030a` on 2026-10-14; evidence below describes the dirty source candidate, not a sealed target artifact.

The framer formerly cleared a trailing `0xB5` when noise contained no complete UBX preamble. A following chunk beginning `0x62` therefore lost a complete valid packet. Preserve that one-byte partial prefix while dropping noise. Packet ordering, checksum rejection and reset behavior remain unchanged. This defect also exists in the original Python framer; it is not characterized as a Rust regression.

The actual tracked framer failed all six standalone regression cases before the patch; its copied candidate passed all six. Five corresponding crate integration tests cover every two-chunk boundary, noise, bytewise delivery, invalid checksum/resynchronization and reset. The sixth standalone case compares the complete existing corpus. No physical device was accessed.

The existing 39-chunk corpus begins with `garbage+B5`, then the remaining valid NAV-PVT packet at the same observation time. The unchanged original decoder, fed only those first two chunks independently reassembled, supplies the corrected oracle. The checker requires the exact known frame delta, preserves all 57 previous ordered frames and all decoded fields, and sends the original split input to the native executable. Neither the fixture nor original decoder was changed, and native output does not generate expectations. Expected counts deliberately change from 57 frames/17 publications to 58/18, recovering one gpsLocationExternal publication.

Validation (canonical environment, coordinated Cargo cache, jobs=2, incremental disabled; disk guards passed before builds):

- `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-ublox`: one existing corpus unit test and five framing integration tests pass.
- `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-ublox --bins --examples`: pass.
- `cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-ublox --all-targets -- -D warnings`: pass after removing two unnecessary clones in the new tests.
- `test_ublox_framing_delta.py`: 13 pass, including rejection of the old trace, lost recovered packet, changed old/recovered fields, reordering, raw bytes/checksum, prefix/final buffers, timestamps, extra chunks and unknown fields.
- `check_ublox.py`: actual native decoder exactly matches independent corrected original on all 39 chunks, 58 frames and 18 publications.
- `check_pigeon.py`: nine actual-source policy scenarios pass.
- `check_ublox_daemons.py`: original/native messages equal (30 ubloxGnss, eight gpsLocationExternal); two serial initializations, existing descriptor reuse after serial pathname removal, assistance requests and shutdown pass (observed stop 0.00355 s).
- `check_ublox_serial.py`: original/native PTY results match; unchanged ASan/UBSan fixture now builds and passes with leak detection and halt-on-error. The first attempt exposed missing host Clang runtime archives. After a separate disk guard, the parent installed the matching Ubuntu `libclang-rt-18-dev=1:18.1.3-1ubuntu1` package into the local SDK (SHA256 `82d5d27d12c0abdf4cefbafa6044e7544a4cbbbec9c04784f5ce326979842b2b`) and reran the entire serial gate successfully. No test was skipped or weakened.

The GNSS CI job installs pinned `pytest==9.0.2` in its own dependency step before the new negative controls. Existing decoder, Pigeon, serial and daemon gates remain required. Evidence is ignored local data under `.analysis/scratch/2026-10-14-ublox-317/`, especially `full-validation/`. Full workspace/cloud checks, target rebuild/seal, physical GNSS behavior and vehicle comparison remain separate gates. No CPU benefit is claimed.
