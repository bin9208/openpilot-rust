#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import json
from pathlib import Path
import sys


def prepare(root: Path, webhook: str) -> None:
  cloud = root / 'cloudflared'
  cloud.write_text(
    '\n'.join(
      [
        f'#!{sys.executable}',
        'import os,sys,json,time,signal',
        'from pathlib import Path',
        f'root=Path({str(root)!r})',
        'if (root/"hold-term").is_file(): signal.signal(signal.SIGTERM,lambda *_:(root/"cloud-term").write_text("TERM"))',
        'stat=Path(f"/proc/{os.getpid()}/stat").read_text().rsplit(")",1)[1].split()',
        '(root/"cloud-child.json").write_text(json.dumps({"pid":os.getpid(),"starttime":int(stat[19]),"argv":sys.argv[1:],"sid":os.getsid(0)}))',
        'print("owned tunnel status",file=sys.stderr,flush=True)',
        'print("https://owned-support.trycloudflare.com",flush=True)',
        'time.sleep(300)',
      ]
    )
    + '\n'
  )
  cloud.chmod(0o700)
  git = root / 'bin/git'
  git.write_text(
    '\n'.join(
      [
        f'#!{sys.executable}',
        'import sys,json,os',
        'from pathlib import Path',
        f'root=Path({str(root)!r})',
        'with (root/"metadata-git.jsonl").open("a") as log: log.write(json.dumps({"argv":sys.argv[1:],"cwd":os.getcwd()})+"\\n")',
        'print("owned-branch" if sys.argv[1]=="branch" else "abc1234" if sys.argv[1]=="rev-parse" else "2026-01-02 03:04:05")',
      ]
    )
    + '\n'
  )
  git.chmod(0o700)
  (root / 'support-environment.json').write_text(
    json.dumps(
      {
        'CARROT_CLOUDFLARED_BIN': str(cloud),
        'CARROT_SUPPORT_AUTO_INSTALL_CLOUDFLARED': '0',
        'CARROT_SUPPORT_DISCORD_WEBHOOK_URL': webhook,
        'CARROT_SUPPORT_DISCORD_WEBHOOK_DISABLE': '0',
        'CARROT_DEVICE_SERIAL': 'owned-support-serial',
        'CARROT_DEVICE_NAME': 'owned-support-device',
        'CARROT_REPO_DIR': str(root / 'repository'),
      }
    )
  )
