"""Re-link the actual original-source build command with one diagnostic wrapper."""
from pathlib import Path
import json
import subprocess
import sys

command, wrapper, output = map(lambda item: Path(item).resolve(), sys.argv[1:4])
record = json.loads(command.read_text())
assert record['exit_code'] == 0
argv = record['argv'].copy()
assert argv[0] == 'clang++'
assert argv.count('-o') == 1
old_output = Path(argv[argv.index('-o') + 1])
assert old_output.exists()
assert argv.count(str(old_output)) == 1
assert sum(arg.endswith('/openpilot/common/swaglog.cc') for arg in argv) == 1
assert sum(arg.endswith('/libzmq.a') for arg in argv) == 1
assert output != old_output and not output.exists()
argv[argv.index('-o') + 1] = str(output)
archive = next(index for index, arg in enumerate(argv) if arg.endswith('/libzmq.a'))
argv.insert(archive, str(wrapper))
argv.insert(1, '-Wl,--wrap=zmq_send')
result = subprocess.run(argv, capture_output=True, timeout=600)
(output.parent / 'wrapped-build.json').write_text(json.dumps({
  'argv': argv, 'returncode': result.returncode,
  'stdout': result.stdout.decode(errors='replace'),
  'stderr': result.stderr.decode(errors='replace')}, indent=2) + '\n')
result.check_returncode()
