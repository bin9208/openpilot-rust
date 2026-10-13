from __future__ import annotations

import asyncio
import base64

from carrot_server_dashcam_http.keepalive import read_response
from carrot_server_dashcam_http.fixture import save
from .fixture import Fixture

async def observe(port: int,path: str):
  reader,writer=await asyncio.open_connection('127.0.0.1',port)
  request=f'GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n'.encode();writer.write(request);await writer.drain();response=await read_response(reader)
  eof=False;probe=None
  try:probe=await asyncio.wait_for(reader.read(1),.25);eof=probe==b''
  except TimeoutError:pass
  follow=None;error=None
  try:
    writer.write(b'GET /api/dashcam/replay-source/owned--0 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n');await writer.drain();follow=await read_response(reader)
  except (asyncio.IncompleteReadError,ConnectionError,TimeoutError) as failure:error={'kind':type(failure).__name__,'text':str(failure)}
  finally:
    writer.close()
    try:await writer.wait_closed()
    except ConnectionError:pass
  return {'request_base64':base64.b64encode(request).decode(),'response':response,'eof_after_response':eof,'probe_base64':base64.b64encode(probe).decode() if probe is not None else None,'follow_up':follow,'follow_up_error':error}

async def failure_recovery(f: Fixture):
  f.file('owned--0','rlog.zst',b'owned raw error/recovery file');f.file('owned--0','qcamera.mp4',b'owned replay marker')
  path='/api/dashcam/download/owned--0/rlog'
  await f.pair('invalid-mime-provider-unknown-kind-skips-lookup','/api/dashcam/download/owned--0/unknown')
  expected=await observe(f.source_port,path);actual=await observe(f.native_port,path)
  def meaningful(value):return {'response':{key:value['response'][key] for key in ('status','headers','body_base64')},'eof_after_response':value['eof_after_response'],'follow_up_status':value['follow_up']['status'] if value['follow_up'] else None}
  row={'scenario':'unhandled-owned-mime-read-error-persistent-connection','source':expected,'native':actual,'compared_source':meaningful(expected),'compared_native':meaningful(actual),'equal':meaningful(expected)==meaningful(actual)}
  f.rows.append(row)
  if not row['equal']:f.failures.append(row)
  save(f.output/'keepalive-error-observation.json',row)
  f.mime_files[0].write_text('application/x-owned-recovered zst\n')
  await f.pair('unhandled-mime-error-fresh-connection-recovery',path)
