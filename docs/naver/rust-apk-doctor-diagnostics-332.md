# APK toolkit doctor failure diagnostics (#332)

Issue: https://github.com/bin9208/openpilot-rust/issues/332

Integration [run38604099866 job115990436901](https://github.com/bin9208/openpilot-rust/actions/runs/38604099866/job/115990436901) at source d6cf1603 failed two existing doctor success assertions:468 tests passed,2 failed in138.62s. Pytest's abbreviated CompletedProcess representation omitted the failed requirement, so this evidence cannot identify its cause. The trailing successful37.2-beta3 version is the selected Android platform requirement, not proof of an adb failure. Previous run job115942463190 passed470 tests in114.79s with the same runner image20261004.327.1, Temurin21.0.12+1, cmdline-tools22.0 and setup configuration; a newly changed toolchain is therefore not established.

The change adds failed requirement names and typed detail strings to the two existing normal/recovery returncode assertions. All returncode, overall success, per-requirement success, usable path/version, empty-SDK rejection, recovery and artifact-free checks remain unchanged. It does not dump stdout, stderr or environment. The next failing exact-SHA run must retain the actual failed name/detail before selecting any production fix; no pin/parser/timeout relaxation is justified yet.

Ignored one-off negative proof executed the unchanged and proposed test functions with an owned fake CLI returning d8/version-probe timeout. Both assertions still failed; old assertion message was empty, proposed message retained `[('d8', 'version probe timed out after 5s')]`. Patch application check passed. No SDK install, Cargo build or device access was used. The patch/document were prepared only in ignored scratch while the clean target build ran; the patch was applied only after all11 clean-source target build groups completed. Fresh exact-SHA cloud validation remains required.

Final tracked verification invoked both modified functions against an owned
failed-doctor response: both still fail with the exact d8/typed-timeout detail,
and an unrelated sentinel field is excluded. The full Ruff invocation reports
an inherited TID251 import at line136; the unchanged HEAD file reproduces the
same warning. Targeted Ruff with only that existing rule excluded passes; no
repository lint configuration/import policy was changed. Native documentation
validation and diff checks pass. Full SDK execution remains the cloud gate.
