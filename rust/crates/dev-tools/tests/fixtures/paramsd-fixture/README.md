# Paramsd fixture conversion evidence

The native generator ports the unchanged `rust/tools/paramsd_fixture.py` at
SHA-256 `8c0356bb780039c475def68358567489e036ab422921b8389bc7fbed20635521`
and the input construction in `rust/tools/check_paramsd_daemon.py` at SHA-256
`04a5433f356d594d3db8ce42394274e0bb1c856d20e975c5657285ce8f998b87`.
These are source pins, not claims of byte parity. The Rust API covers the
service/data shapes exercised by the seven scenario cases and the two daemon
inputs. It does not replace Python `event(service, data)` for arbitrary Cap'n
Proto services or dictionaries. The Python helpers and their callers remain
until cloud comparison verifies every generated Cap'n Proto byte.

Run `generate-paramsd-fixture --root ROOT --output NEW_DIR` to create
`scenarios.json` and `continuous-input.json`. After the unchanged Python daemon
checker writes its actual `continuous/saved.bin`, run with
`--saved-packet PATH` into another new directory to create those two files and
`restart-input.json`. Output directories must not already exist. The saved
packet is original-observed persistence, never synthesized by the generator.

For the mandatory cloud parity test, capture original-only inputs from the
unchanged Python helpers into a fresh evidence directory. This snippet runs
after the unchanged Python controller has produced `continuous/saved.bin`:

```python
import json
from pathlib import Path
from paramsd_fixture import car_params, scenarios
from check_paramsd_daemon import frames

evidence = Path('EVIDENCE_DIRECTORY')
saved = (evidence / 'continuous/saved.bin').read_bytes()
(evidence / 'original-saved.bin').write_bytes(saved)
(evidence / 'original-scenarios.json').write_text(json.dumps({'cases': scenarios()}) + '\n')
for restart in (False, True):
    gps = 'gpsLocation' if restart else 'gpsLocationExternal'
    seed = {'LiveParametersV2': list(saved), 'CarParamsPrevRoute': car_params()} if restart else {}
    case = {'name': 'restart' if restart else 'continuous', 'car': car_params(),
            'seed': seed, 'debug': True, 'replay': True, 'simulation': True,
            'gps': gps, 'frames': frames(gps, 8 if restart else 1202)}
    case['frames'].insert(0, {'time': 99.95, 'messages': []})
    (evidence / ('original-restart.json' if restart else 'original-continuous.json')).write_text(json.dumps(case) + '\n')
```

Set `PARAMSD_FIXTURE_ROOT`, `PARAMSD_FIXTURE_ORIGINAL_SAVED_PACKET`,
`PARAMSD_FIXTURE_ORIGINAL_SCENARIOS`, `PARAMSD_FIXTURE_ORIGINAL_CONTINUOUS`,
`PARAMSD_FIXTURE_ORIGINAL_RESTART`, `PARAMSD_FIXTURE_NATIVE_OUTPUT`,
`PARAMSD_FIXTURE_NATIVE_EXEC`, and `PARAMSD_FIXTURE_NATIVE_BINARY` for the
focused Rust test. The native output must come from a prior, successful cloud
CLI invocation under strace, with exact arguments `--root ROOT --output
NATIVE_OUTPUT --saved-packet ORIGINAL_SAVED_PACKET`. The test requires one
successful `execve` and no other exec attempt. It compares the parsed full JSON
objects, including every wire-byte array and float bit array, against
independently constructed original inputs, and compares CLI and library files
byte-for-byte. Preserve the actual Python files, captured inputs, original
controller outputs, source hashes and native generator executable hash in the
cloud artifact.
