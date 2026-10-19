# Params Float32 controller contract

The 25 raw inputs in `inputs.json` are a source transcription from unchanged
`rust/tools/check_calibration_float.py` (SHA256
`922e055f3e9b8fda4e732e83b5332105b5203bd9f56964bf5515e3557c9ff304`).
They are not captured output. No local execution was performed for this unit.

The mandatory cloud comparison runs that unchanged checker and the Rust
controller against the same current `calibration_float` example. It compares
all report fields, the success line and the complete generated C++ translation
unit. Both independently extract `Params::getFloat` from the retained C++ header
and compile it with `g++ -std=c++17 -O2`. The Rust controller never imports the
candidate's float parser or uses a result fixture as its source oracle.

The original has unbounded child waits. Native compiler/probe deadlines are
60/5 seconds, an explicit safety improvement, with owned group cancellation.
Raw stdout/stderr remain in the exclusively created output directory on failure.
Success publication uses no-clobber files, held-inode rollback, fallible stdout,
and before/after source and executable identity checks. This is bounded identity
validation, not an atomic execution snapshot against arbitrary ABA mutation.

The original checker remains required by the cloud comparison until retirement
is separately reviewed. Production provenance allows only its genuine absence
as a historical source; the retained `openpilot/common/params.h` remains required.
Altered originals, dangling links, input replacement and source drift fail.
No Python source is archived or renamed into this fixture directory.

Source-review follow-up adds an actual cloud original/native control with PATH
removed versus explicitly empty. Missing PATH uses CPython's POSIX default
`/bin:/usr/bin`; empty PATH still searches the private cwd and fails there when
no compiler exists. A lookup-only unit also verifies a private cwd compiler is
selected only for the empty case. Another unit latches cancellation during
handler removal and requires pending owned receipts to roll back while a
foreign replacement survives. Publication descriptors stay held until this
final flag check. The FIFO-specific unit is Linux-gated because rustix does not
provide mkfifoat on Apple targets. These additional controls are source-ready,
not locally executed: Linux selection is now 12 integration plus 4 module tests.
