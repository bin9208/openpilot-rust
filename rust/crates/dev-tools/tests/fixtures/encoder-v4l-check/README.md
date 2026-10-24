# Original V4L checker contracts

Seventeen captures run unchanged check_encoder_v4l.py main, source extraction,
trace/count/configuration/metadata checks, outcome rejection and filesystem cleanup.
External process/disk/PID/shm-root boundaries are controlled; actual V4L hardware is
not used. Success executes two build and64comparison commands across32cases.
Expected child aborts require both original status and diagnostic; sanitizers fail.
The original source SHA is in the corpus. Actual32C++/Rust host driver cases are
required separately before retiring the original checker. No replacement existed
when captured, and production never reads this corpus.

Actual current C++ and Cargo-selected Rust driver probes passed32cases. Running
original and native checkers at the same absolute path yielded byte-identical full
receipts (including compiled source/driver SHA256). Nineteen separate unchanged
encoder_outcomes captures preserve status, diagnostic and sanitizer admission.
