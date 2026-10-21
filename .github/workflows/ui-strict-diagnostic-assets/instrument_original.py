"""Cloud-only diagnostic copy of the retired, independently captured UI driver."""
import hashlib
import sys
from pathlib import Path

source, target = map(Path, sys.argv[1:3])
raw = source.read_bytes()
assert hashlib.sha256(raw).hexdigest() == '1b457b164f000e8efc06ff3cd28e00ad05b3fa6f6c9890881567cda50f08679b'
text = raw.decode()
needle = 'import pyray as rl\n'
assert text.count(needle) == 1
hook = '''import pyray as rl
# Diagnostic-only observation of the real pyray GetFPS result. The strict
# comparison in unchanged application.py still receives the same integer.
import atexit, time
_fps_samples = []
_real_get_fps = rl.get_fps
def _record_get_fps():
  fps = _real_get_fps()
  _fps_samples.append({'frame': len(_fps_samples) + 1, 'fps': fps, 'monotonic_ns': time.monotonic_ns()})
  return fps
rl.get_fps = _record_get_fps
_real_exit = os._exit
def _save_fps():
  Path(os.environ['UI_APPLICATION_FPS_DIAGNOSTIC_OUTPUT']).write_text(json.dumps(_fps_samples, indent=2) + '\\n')
def _observed_exit(code):
  _save_fps()  # Called only after the unchanged strict branch has decided.
  _real_exit(code)
os._exit = _observed_exit
atexit.register(_save_fps)
'''
target.write_text(text.replace(needle, hook))
