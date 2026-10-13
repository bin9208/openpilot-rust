# Native UI hardware selection (#279)

Issue: https://github.com/bin9208/openpilot-rust/issues/279

The first C3X run of `9200ffe57c385e2fa4a89ff11821c7e9ac710d2f` selected
the compact UI on its 2160x1080 display. The original main UI selects its
layout through `GuiApplication.big_ui()`, which includes tici/tizi hardware
and the explicit `BIG=1` override. Original font selection and scaling use
the separate environment-only `system/ui/lib/application.py::BIG_UI` flag.

`startup-ui::Config::for_runtime()` already preserves that distinction:
`large_viewport` includes tici/tizi hardware; `big` reflects only `BIG=1`.
The native application incorrectly bound both `Context.big` and
`DeviceConfig.big` to `graphics.big`. Consequently, `Main::new()` selected
the compact layout and the device controller selected the five-second
ignition timeout instead of the original large-UI ten-second timeout.

Both native bindings now use `graphics.large_viewport`. The renderer still
uses `graphics.big` for fonts. Mici/C4 remains compact with `BIG` unset;
desktop `BIG=1` still selects the large UI. No settings, control or
scheduling policy was changed.

Focused host evidence is under `.omo/evidence/279-native-ui-selection/`.
`build-result.json` records the bounded host build and reserve check.
`validate-host.py` runs the existing Rust tests, Clippy and source/native
state, root, device, application and product-runtime checks; command files
record the exact invocations and environments. All nine checks passed:
formatting, 29 focused Rust tests, the current state-trace build, Clippy,
2,400 source/native UIState/Device frames, 20 root scenarios with zero pixel
or state differences, 64 settings scenarios with exact traces and final
pixels, six application lifecycle scenarios and four native product-runtime
scenarios. `host-validation-result.json` and each scenario's result/log
retain these outcomes. `EVIDENCE.md` maps the criteria to commands and
captured observables.

The parent integration session also built the focused AGNOS UI target
successfully in 108.377 seconds, then ran that binary temporarily on the
physical C3X under the original UI process's environment and working
directory as the comma account. With `BIG` unset, the corrected native UI
selected the large C3 sidebar and main cards and remained alive during a
90-second offroad probe. The private original/corrected active DRM scanout
captures visibly distinguish the compact and large layouts. The original
installed UI was restored afterward; the captured restoration state retains
`DisableDM=2` and `IsOnroad=0`. Build and physical receipts are
`target-build-result.json`, `physical-ui-result.json` and
`physical-restored-state.txt` in the same evidence directory. Raw captures
remain private and outside Git.

The host fixtures inject their contexts and do not exercise the native
hardware binding. Their preservation results are separate from the physical
offroad probe. The probe did not activate a modified runtime package. The
PR, exact-merge CI and matching runtime release assets remain acceptance
gates owned by the parent integration session. The issue must remain open
until those criteria are satisfied. This record claims no driving or CPU
improvement.
