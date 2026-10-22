# GitHub Actions runner comparison — issue #496

The owner requested a direct speed and reliability comparison on 2026-10-23 KST.
This isolated experiment does not migrate production CI routing or integrate code.

- Issue: https://github.com/bin9208/openpilot-rust/issues/496
- Fixed source: `dc95138076836ea3a5c1b6c27262be3f3b912f71` (`dev-md`).
- Runners: GitHub-hosted `ubuntu-24.04`, registered `bin-actions` (`self-hosted/Linux/X64`).
- Rust1.94.0, two build jobs, no incremental compilation, debug information disabled.
- Fresh per-job Cargo/Python/Gradle caches; separate clean targets for three rounds.
- Existing CI/wiki/Jetlink Python checks, Android core/runtime tests with rerun-tasks,
  runtime-core and msgq tests including actual native C++ IPC peers.
- Separate environment, source/submodule checkout, package/tool setup and timed stages.
- FFmpeg6.1.1 pins are checked by APT simulation only; no FFmpeg install is claimed.
- This is focused host CI evidence, not full ARM/AGNOS/release or device validation.
- Local validation: YAML parse, every embedded Bash syntax check, and source review.
  Functional tests/builds run only in GitHub Actions for the explicit comparison.

## Results

Pending the paired Actions experiment. Keep failures and first-attempt outcomes
visible; distinguish warm reruns from clean compilation and network setup.
