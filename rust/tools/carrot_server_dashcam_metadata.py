#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import asyncio
from pathlib import Path
import sys

from carrot_server_dashcam_http.fixture import save
from carrot_server_dashcam_metadata.fixture import Fixture,prepare
from carrot_server_dashcam_metadata.scenarios import app,default_mime,metadata,populated,raw
from carrot_server_dashcam_metadata.sockets import failure_recovery

async def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--composed-only',action='store_true');args=parser.parse_args()
  output=args.output.resolve();binary=args.binary.resolve();prepare(binary,output,sys.argv)
  cases=('application','mime-error') if args.composed_only else ('defaults','metadata-and-files','mime-error');rows=[];failures=[];exits=[]
  for case in cases:
    directory=output/case;directory.mkdir();mime_files=[]
    if case!='defaults':
      mime=directory/'owned-mime.types';mime.write_bytes(b'\xff' if case=='mime-error' else b'application/x-owned-ts ts\r\napplication/x-owned-log zst # trailing\rapplication/x-owned-video\x1cmp4\napplication/ignored bz2 MP4\n');mime_files=[mime]
    fixture=Fixture(binary,directory,mime_files,args.composed_only)
    if case in ('metadata-and-files','application'):populated(fixture)
    await fixture.start()
    try:
      if case=='defaults':await default_mime(fixture)
      elif case=='mime-error':await failure_recovery(fixture)
      elif case=='application':await app(fixture)
      else:await metadata(fixture);await raw(fixture)
    finally:
      await fixture.close();save(directory/'observations.json',fixture.rows);save(directory/'failures.json',fixture.failures)
    rows+=fixture.rows;failures+=fixture.failures;exits.append(fixture.native.returncode)
  save(output/'result.json',{'pairs':len(rows),'failures':len(failures),'native_exit_codes':exits,'surface':'unchanged original source functions/aiohttp FileResponse and native localhost handlers with owned files/MIME provider',
    'observables':'raw selected header bytes/body bytes/status; persistent unhandled500 EOF/followup and fresh recovery; no parsing/decompression/ffmpeg/upload',
    'limits':['shared full range/conditional protocol and unchanged92catalog+74list proofs reused','only allowed MIME filenames/system mapping adapter; owned provider overrides/defaults measured','full runtime/device/NAS gates remain open']})
  if failures or any(exits):raise SystemExit(1)

if __name__=='__main__':asyncio.run(main())
