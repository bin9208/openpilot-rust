#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import shutil
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  assert shutil.disk_usage(root).free >= 26 * 1024**3, '25GiB reserve +1GiB build growth required'
  args.output.parent.mkdir(parents=True, exist_ok=True)
  sources = ['ipc.cc', 'event.cc', 'impl_msgq.cc', 'impl_fake.cc', 'msgq.cc', 'visionipc/visionipc.cc', 'visionipc/visionipc_client.cc', 'visionipc/visionipc_server.cc', 'visionipc/visionbuf.cc']
  command = [os.environ.get('CXX', 'c++'), '-std=c++17', '-O1', '-pthread', '-UNDEBUG', '-I', str(root / 'msgq_repo'), *[str(root / 'msgq_repo/msgq' / source) for source in sources], str(root / 'rust/crates/athena/tests/native/vision_peer.cc'), '-o', str(args.output)]
  subprocess.run(command, check=True)
  print('PASS: owned original-VisionIPC peer compiled', args.output)


if __name__ == '__main__':
  main()
