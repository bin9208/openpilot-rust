# /// script
# requires-python = ">=3.12"
# dependencies = ["aiortc==1.14.0", "av==16.1.0", "aiohttp==3.13.3", "dnspython==2.8.0"]
# ///
# Run with the pinned source-oracle environment and owned output/IPC roots.
"""Invoke unchanged original CLI with its actual Cython Params provider."""

import os
from pathlib import Path
import sys

from original_params_binding import load

root = Path(os.environ['WEBRTC_OWNED_ROOT']).resolve(strict=True)
params = Path(os.environ['PARAMS_ROOT'])
assert params.parent.resolve() == root and os.environ['OPENPILOT_PREFIX']
load(Path(os.environ['WEBRTC_PARAMS_BINDING']), f'ipc://{root}/logs.sock', root / 'logs')
from openpilot.common.params import Params

key = Path(Params().get_param_path('CarrotVisionActive'))
assert key.parent.resolve().is_relative_to(params.resolve())
carrot = sys.argv.pop(1) == 'carrot'
if carrot:
  from openpilot.system.webrtc.carrot_webrtcd import main
else:
  from openpilot.system.webrtc.webrtcd import main
main()
