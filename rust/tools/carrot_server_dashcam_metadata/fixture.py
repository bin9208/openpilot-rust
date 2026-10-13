from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
from typing import Any

import aiohttp
from aiohttp import web
from yarl import URL

from carrot_server_dashcam_http.fixture import save
from .source import application, source

HEADERS = frozenset(('content-type','content-length','cache-control','content-disposition','x-content-type-options',
  'etag','last-modified','accept-ranges','content-range','allow','content-encoding','vary'))

class Fixture:
  def __init__(self,binary: Path,output: Path,mime_files: list[Path],composed: bool=False):
    self.binary=binary;self.output=output;self.root=output/'owned-segments';self.root.mkdir()
    self.mime_files=mime_files;self.composed=composed;self.rows=[];self.failures=[]

  def file(self,segment: str,name: str,body: bytes,mtime_ns: int=1700000000123456789) -> Path:
    directory=self.root/segment;directory.mkdir(exist_ok=True);path=directory/name;path.write_bytes(body);os.utime(path,ns=(mtime_ns,mtime_ns));return path

  async def start(self):
    routes,proof=source(self.root,self.output/'source-state'/'state.json',self.mime_files);save(self.output/'original-function-provenance.json',proof)
    self.runner=web.AppRunner(application(routes));await self.runner.setup();site=web.TCPSite(self.runner,'127.0.0.1',0);await site.start();self.source_port=site._server.sockets[0].getsockname()[1]
    self.native=await asyncio.create_subprocess_exec(str(self.binary),stdin=asyncio.subprocess.PIPE,stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.PIPE)
    config={'root':str(self.root),'state':str(self.output/'native-state'/'state.json'),'wall':1700001000,'mime_files':[str(path) for path in self.mime_files],'composed':self.composed,'app_state':str(self.output/'owned-app-state')}
    self.native.stdin.write((json.dumps(config)+'\n').encode());await self.native.stdin.drain();initial=await asyncio.wait_for(self.native.stdout.readline(),10)
    if not initial: raise RuntimeError('native fixture failed to start')
    self.native_port=json.loads(initial)['port'];self.client=aiohttp.ClientSession(auto_decompress=False,skip_auto_headers={'Accept-Encoding'})
    save(self.output/'owned-listeners.json',{'source_port':self.source_port,'native_port':self.native_port,'native_config':config})

  async def response(self,port: int,path: str,method: str,headers: dict[str,str]):
    async with self.client.request(method,URL(f'http://127.0.0.1:{port}'+path,encoded=True),headers=headers) as response:
      body=await response.read()
      selected={name.decode().lower():base64.b64encode(value).decode() for name,value in response.raw_headers if name.decode().lower() in HEADERS}
      return {'status':response.status,'headers_base64':selected,'body_base64':base64.b64encode(body).decode()}

  async def pair(self,scenario: str,path: str,method: str='GET',headers: dict[str,str]|None=None):
    headers=headers or {};expected=await self.response(self.source_port,path,method,headers);actual=await self.response(self.native_port,path,method,headers)
    row={'scenario':scenario,'request':{'method':method,'path':path,'headers':headers},'source':expected,'native':actual,'equal':expected==actual};self.rows.append(row)
    if not row['equal']:self.failures.append(row)
    return expected

  async def close(self):
    await self.client.close();self.native.stdin.write(b'\n');await self.native.stdin.drain();stdout,stderr=await asyncio.wait_for(self.native.communicate(),10)
    (self.output/'native-stdout.txt').write_bytes(stdout);(self.output/'native-stderr.txt').write_bytes(stderr);await self.runner.cleanup()

def prepare(binary: Path,output: Path,argv: list[str]):
  output.mkdir(parents=True,exist_ok=True);free=shutil.disk_usage(output).free;growth=8*1024**2
  save(output/'diskguard.json',{'free_bytes':free,'reserve_bytes':25*1024**3,'estimated_growth_bytes':growth})
  if free<25*1024**3+growth:raise RuntimeError('disk reserve requires recovery')
  save(output/'invocation.json',{'command':[sys.executable,'-P',*argv],'PYTHONPATH':os.environ.get('PYTHONPATH',''),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()})
  paths=[Path('openpilot/selfdrive/carrot/server/features/dashcam')/name for name in ('routes.py','summary_sources.py','catalog.py','paths.py')]
  save(output/'source-hashes.json',{str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in paths})
