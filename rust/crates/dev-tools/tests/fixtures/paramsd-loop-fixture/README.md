# Paramsd loop input conversion

`rust/tools/paramsd_loop_fixture.py` is retained as the independent original
input source. Its SHA-256 is
`767f58edf7163edf8eaa493b5453f3bc00f6966772dba932e96a294e3efe3089`;
the imported `rust/tools/paramsd_fixture.py` is
`8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521`.
The native generator checks both source pins before and after generation. It
creates all 39 cases: 22 legacy migration encodings, 12 V2 cache variants,
three malformed/suppression cases, and two ordered 1,210-frame GPS lanes.
No native output is an expected oracle.

On GitHub Actions, capture the unchanged original inputs in a new evidence
directory before invoking the native generator:

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

The parent workflow adds a `generate-paramsd-loop-fixture` CLI using the new
leaf module and runs it once as `--root "$GITHUB_WORKSPACE" --output
"$PARAMSD_LOOP_EVIDENCE/native"` under `strace -f -qq -s 65535 -e
trace=execve,execveat`, recording the full trace. The output directory must
not already exist. Set `PARAMSD_LOOP_ROOT`, `PARAMSD_LOOP_ORIGINAL_INPUT`,
`PARAMSD_LOOP_NATIVE_OUTPUT`, `PARAMSD_LOOP_NATIVE_EXEC`, and
`PARAMSD_LOOP_NATIVE_BINARY` for the mandatory Rust test. It requires exactly
that one native executable, compares the complete parsed 39-case input tree,
including every raw Cap'n Proto byte and message order, against the fresh
original, and compares library/CLI files byte-for-byte. Preserve original
input, native input, both source hashes, trace, and binary hash in the cloud
artifact. The historical Python source and `check_paramsd_loop.py` remain
until the caller and absence gates are separately proved.
