import json
import sys

import requests
from openpilot.selfdrive.carrot.web_upload import create_web_upload_session_sync

config = json.loads(sys.stdin.readline())
try:
  result = create_web_upload_session_sync(config['base'], config['metadata'], requests.post, 'tmux')
  output = dict(result=result)
except Exception as error:
  output = dict(error_type=type(error).__name__, error=str(error))
print(json.dumps(output), flush=True)
