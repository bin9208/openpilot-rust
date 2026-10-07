import json
import sys
import time
from pathlib import Path
import msgq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST

context = msgq.Context()
socket = msgq.SubSocket()
socket.connect(context, b'selfdriveState', segment_size=SERVICE_LIST['selfdriveState'].queue_size)
output = Path(sys.argv[1])
rows = []
print('READY', flush=True)
while True:
  payload = socket.receive(non_blocking=True)
  if payload is not None:
    with log.Event.from_bytes(payload) as event:
      rows.append({'valid': event.valid, 'time': event.logMonoTime, 'sound': str(event.selfdriveState.alertSound)})
  elif output.with_suffix('.stop').exists():
    break
  else:
    time.sleep(0.001)
output.write_text(json.dumps(rows, indent=2))
