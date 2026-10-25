# Paramsd loop input conversion

`rust/tools/paramsd_loop_fixture.py` supplied the independent original input
source for the accepted pre-deletion capture. Its SHA-256 is
`767f58edf7163edf8eaa493b5453f3bc00f6966772dba932e96a294e3efe3089`;
the imported `rust/tools/paramsd_fixture.py` is
`8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521`.
The native generator requires the historical loop fixture path to be absent
after deletion, and checks the retained `paramsd_fixture.py` plus its Rust
source pins before and after generation. It
creates all 39 cases: 22 legacy migration encodings, 12 V2 cache variants,
three malformed/suppression cases, and two ordered 1,210-frame GPS lanes.
No native output is an expected oracle.

The pre-deletion GitHub Actions capture invoked the unchanged original inputs
in a fresh evidence directory before the native generator:

```sh
mkdir -p "$PARAMSD_LOOP_EVIDENCE"
PYTHONPATH="$GITHUB_WORKSPACE/rust/tools:$GITHUB_WORKSPACE" python3 - <<'PY'
import json
import os
from pathlib import Path
from paramsd_loop_fixture import cases

path = Path(os.environ['PARAMSD_LOOP_EVIDENCE']) / 'original-input.json'
path.write_text(json.dumps({'cases': cases()}) + '\n')
PY
sha256sum rust/tools/paramsd_loop_fixture.py rust/tools/paramsd_fixture.py "$PARAMSD_LOOP_EVIDENCE/original-input.json" > "$PARAMSD_LOOP_EVIDENCE/original-input.sha256"
```

The parent workflow runs the `generate-paramsd-loop-fixture` CLI using the
native leaf module as `--root "$GITHUB_WORKSPACE" --output
"$PARAMSD_LOOP_EVIDENCE/native"` under `strace -f -qq -s 65535 -e
trace=execve,execveat`, recording the full trace. The output directory must
not already exist. Set `PARAMSD_LOOP_ROOT`, `PARAMSD_LOOP_ORIGINAL_INPUT`,
`PARAMSD_LOOP_NATIVE_OUTPUT`, `PARAMSD_LOOP_NATIVE_EXEC`, and
`PARAMSD_LOOP_NATIVE_BINARY` for the mandatory Rust test. It requires exactly
that one native executable, compares the complete parsed 39-case input tree,
including every raw Cap'n Proto byte and message order, against the sealed
original, and compares library/CLI files byte-for-byte. Preserve original
input, native input, retained source hashes, trace, and binary hash in the cloud
artifact. For the post-deletion gate, decompress the authenticated original-only
`paramsd-loop-retired-original/input.json.bz2` into a fresh cloud output path
and pass that path as `PARAMSD_LOOP_ORIGINAL_INPUT`. The two historical Python
paths must be absent from the actual checkout and copied-root lane. The parent
owns deletion, caller migration, and workflow registration.
