# JSON11 staging raw-path defect (#452)

Original-only hosted run39554286034 used a valid build-messages file and an output
directory containing byte0xff. The Python Panda controller admitted the path,
printed three disk reservations and retained invocation/command/error receipts.
Its first native child stage-json11 panicked with exit101; the controller failed
with1 and wrote no final result. This is an observed failure contract, not a
successful raw-path staging result.

The child starts parsing with std::env::args() in json11_stage.rs:144, which panics
on a non-UTF-8 Unix argument. A complete repair also needs lossless curl -o argv
at line69 and a surrogate-preserving success receipt at line176. Merely changing
the parser leaves lossy path selection and JSON serialization behind.

Preserve archive hash, path/symlink/size admission and disk checks. A focused
hosted regression should stage the pinned wheel to a real raw-byte directory and
verify downloaded/extracted bytes and receipt path round trips. Also retain
normal staging tests/Clippy. No local functional execution or device test was
performed. No repair is included yet: the current Panda parity proof retains the
original observed exit101, and this child defect needs separate validation.

Original artifact ZIP SHA256:
c3425d3db45b5df9296e1421f9736f0af52ba70aa869bc66452266e2d2cc0657.
This is part of #296 and does not affect dev-md/#338.

Tracked independently in [issue #452](https://github.com/bin9208/openpilot-rust/issues/452); controller retirement remains part of #296.
