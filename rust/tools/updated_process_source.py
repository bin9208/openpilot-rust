"""Original updater subprocess helper behind explicit harmless command inputs."""

import json
import subprocess
import sys

from openpilot.system.updated.process import run

config = json.load(sys.stdin)
try:
  value = {'output': run(config['argv'], config.get('cwd')), 'code': 0}
except subprocess.CalledProcessError as error:
  value = {'output': error.output, 'code': error.returncode}
except KeyboardInterrupt:
  value = {'interrupted': True}
print(json.dumps(value))
