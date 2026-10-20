# Bluetooth gesture case generator

The native generator ports the case roster in
`rust/tools/bluetooth_gesture_cases.py` (SHA-256
`66810b7ba9f149785d872ea4f4b5ab8ccdce19d26ba30e065f7fc54f1d388253`)
and reads the unchanged recording
`openpilot/selfdrive/carrot/bluetooth/tests/yiser_j6.json` (SHA-256
`e68fb82c2b4927f02c6fc54363ce92fc443893c86e9ae25a006eb8b8dfc0440f`).
Both inputs are pinned through held regular-file descriptors and checked again
around exclusive output publication. No saved native output is an oracle.

The source-defined roster has 37 cases: twelve shared long-operation cases,
eighteen press-duration boundaries, six double-press boundaries, and one
recording. The long sequence includes 10,000 seeded random iterations. The
generator uses the existing CPython-compatible MT19937 `Random::below`, with
the original choice and rejection-consumption order. The source-level tests
check case order, boundary float bits and integer middle stamps, random operation counts,
recording types,
source drift and output no-clobber. These checks do not prove full Python
equivalence.

The cloud gate must create one fresh `gesture_fixture` executable, run the
unchanged Python `check_bluetooth_gestures.py` with that executable, save its
actual `input.jsonl` and result artifacts, then run this native generator
against the same pinned source tree. Require the **complete JSONL bytes** to
match, including operation order, numeric types and float formatting. Execute
the same candidate on the native input and require its complete stdout/stderr
to match the original checker's candidate capture. The unchanged original
checker must also pass its Python Decoder comparison.
The original Python scripts remain in place until this live comparison passes.
No local functional build, checker or generator was run for this preparation.
