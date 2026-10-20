# Bluetooth enumeration reference after Python retirement

The independent Python 3.12 observation is sealed in
`bluetooth-enumerate/original-result.json`. Run the native
`check-bluetooth-enumerate` in the current checkout and an independently
copied checker-absent source root against the **same freshly built**
`openpilot-bluetooth` `input_fixture` ELF. Give each lane a distinct fresh
output directory and capture raw stdout/stderr separately. The reference
requires `BLUETOOTH_ENUMERATE_{NATIVE,ABSENT}_{OUTPUT,STDOUT,STDERR}` and
`BLUETOOTH_ENUMERATE_ABSENT_ROOT`.

The corpus has the eleven historical named nodes and 29 code points that
CPython 3.12 treats as whitespace: U+0009..U+000D, U+001C..U+0020, U+0085,
U+00A0, U+1680, U+2000..U+200A, U+2028..U+2029, U+202F, U+205F, U+3000.
The independent Rust oracle pins this set and never asks candidate helpers
for expected values. The reference compares both fresh reports, fixture
streams, stdout/stderr, and full device pairs with the sealed original,
normalizing only each output-root prefix. Fresh same-ELF lanes retain exact
source/native and cross-lane enumeration order checks. It does not compare new ELF hashes
with the prior run's different ELF.

The original/present/absent same-ELF comparison passed in Actions run
`39508434006`, job `118923150264`, artifact `12304089348`, before deletion.
The retained original raw report came from run `39499806776`. The native
controller now requires the historical checker absent and rejects any
reappearance through final receipt publication. Its copied-root lane must
retain a bounded exact `execve` audit requiring one controller and two
fixture ELF invocations, without Python or any extra exec attempt. Executable
hashes and source files must be checked before and after both fresh lanes.
The post-deletion cloud gate remains pending.
