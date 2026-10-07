# Training graphics ownership (#148)

The Home/onboarding port adds CPU image decode into owned RGBA bytes, a filtered
GPU upload that retains the existing render-thread release queue, and the original
raylib ring primitive. Foreign image handles stay on the decoding thread. Only
Rust-owned bytes cross the worker boundary; texture creation/destruction remains
on the render thread. The external pinned raylib/GL dependency is unchanged.

`rust/tools/check_ui_training_adapter.py --target TARGET/debug --raylib RAYLIB
--evidence EVIDENCE` builds and executes the C++ adapter with AddressSanitizer and
UndefinedBehaviorSanitizer. Set `DISPLAY` to an owned Xvfb and `LD_LIBRARY_PATH` to
`RAYLIB/lib`; check disk space before building. The captured result records exact
commands and a real GL screenshot. Four window lifecycles decode all 19 original
training PNGs on worker threads, then upload, draw, release and attempt a repeated
release on the render thread: 76 decodes/uploads/rings, exit zero with halt-on-error.
Driver leak detection is disabled; this is not a driver leak-coverage claim.

The paired original/native Home fixture additionally renders the original 19 PNGs
through the production training implementation and verifies every pixel. The
initial English/Korean training runs matched all 42 frames per language, state,
RecordFront writes and completion navigation. Broader onboarding/UI integration,
independent review, ARM build and whole-runtime startup/upload remain separate
gates. No vehicle, camera, NAS or account is used by this host adapter check.
