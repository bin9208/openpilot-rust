"""Limited-frame normal UI shutdown must retain an already successful update's reboot."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import threading
import time

parser=argparse.ArgumentParser()
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--display',required=True)
args=parser.parse_args()
args.output.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parents[2]
class Server(http.server.BaseHTTPRequestHandler):
  def do_HEAD(self):
    time.sleep(0.5)
    self.send_response(503)
    self.end_headers()
  def log_message(self,*a):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Server)
thread=threading.Thread(target=server.serve_forever)
thread.start()
results=[]
try:
  for layout in ['mici','tici']:
    directory=(args.output/layout).resolve()
    directory.mkdir(exist_ok=True)
    updater=directory/'updater'
    updater.write_text('#!/bin/sh\nsleep 0.03\nprintf "verified inactive slot: 100\\n"\nexit 0\n')
    updater.chmod(0o755)
    manifest=directory/'manifest.json'
    manifest.write_text(json.dumps([{'url':f'http://127.0.0.1:{server.server_port}/owned-asset'}]))
    env=dict(os.environ,DISPLAY=args.display,PROFILE_RENDER='2',BIG='0' if layout=='mici' else '1',SCALE='1' if layout=='mici' else '0.25',PARAMS_ROOT=str(directory/'params'),OPENPILOT_ROOT=str(root),OPENPILOT_PREFIX='owned-updater-close-'+layout,DBUS_SYSTEM_BUS_ADDRESS='unix:path=/tmp/owned-updater-close-no-bus-'+str(os.getpid()))
    with (directory/'native.log').open('w') as log:
      process=subprocess.run([str(args.binary.resolve()),str(updater),str(manifest)],cwd=directory,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=8)
    text=(directory/'native.log').read_text()
    result={'layout':layout,'exit':process.returncode,'verified':'verified inactive slot: 100' in text,'reboots':text.count('REBOOT!'),'two_frames':'Rendered 2 frames' in text}
    results.append(result)
    (args.output/'results.json').write_text(json.dumps({'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'cases':results},indent=2))
    print(result,flush=True)
  assert all(r['exit']==0 and r['verified'] and r['reboots']==1 and r['two_frames'] for r in results)
finally:
  server.shutdown()
  thread.join()
  server.server_close()
