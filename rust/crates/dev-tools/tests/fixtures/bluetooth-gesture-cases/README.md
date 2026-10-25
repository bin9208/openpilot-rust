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
The live byte comparison passed at checked-out source
`9845db2db390bc904401a3794b723b510e3bef84`, job
`118921359333` in run `39507901490`. Its retained artifact
`9845-bluetooth.zip` (SHA-256
`a482e760a8aa8ba41d6c29cf16e029915e48ef654be2a6c5fd778c416ad082a3`)
contains the unchanged original helper's complete 37-case input JSONL and
unchanged Python decoder's complete output. `original-result-9845.json` is the
228-byte original report copied without rewriting. Its SHA-256 is pinned by
`original-observation-9845.json`, which records the large JSONL raw hashes,
original result totals (37 cases, 260,012
steps, 1,024 tokens), and source/binary attribution. It contains no relocated
Python source or native-generated oracle. The previous comparison establishes
present-root equivalence only; the historical helper is still retained.

The new consumer contract takes `--input` with JSONL from the native
`generate-bluetooth-gesture-cases` executable. It continues to run the
original Python Decoder against every operation and compare every candidate
step. The combined `check_bluetooth_runtime.py` suite generates this input
once from its configured `--gesture-generator` (defaulting to the binary in
`--bin-dir`), retains its separate generator log/receipt, and passes the file
to its unchanged `gestures` case position among fourteen cases. It has no
Python-helper fallback. The native generator permits only the exact historical
helper to be absent, holding its real parent and, when present, the original regular file;
the recording remains mandatory and pinned. The retirement test requires the
same current generator ELF to run once from the present checkout and once from
a copied root with only that helper omitted, with `PATH=/nonexistent` and
`strace` recording exactly one native `execve` per lane. Each output must match
the fresh original input byte-for-byte and its independently captured raw
hash. The decoder's fresh original output must match the captured raw hash.
A copy of the current checker, model source and package initializers in the
absent root must then consume the absent native input with a private
`PYTHONPATH`. Require its complete input, original decoder output, candidate
output/stderr and result to match the present-root check. The checker must not
import the retired generator. Pin the current generator/candidate ELF and
recording before and after both
lanes. Preserve the current Python helper until this cloud absence proof and
consumer transition pass. No local functional build, checker or generator was
run for this preparation.
