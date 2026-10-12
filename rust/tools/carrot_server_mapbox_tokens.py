# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
import argparse
import anyio
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import threading
import time
from types import ModuleType
from typing import Literal,TypeAlias,TypedDict
import urllib.request

import aiohttp
from aiohttp import web
import brotli
from yarl import URL
from original_params_binding import load

Json:TypeAlias=None|bool|int|float|str|list['Json']|dict[str,'Json']
class Case(TypedDict,total=False):
  scenario:str
  method:Literal['GET','HEAD','POST','DELETE']
  path:str
  body:Json
  raw:str
  seed:str
  mode:str
  response:Json
  response_status:int

PUBLIC='pk.FAKE_OWNED_PUBLIC_012345'
SECRET='sk.FAKE_OWNED_SECRET_012345'
PATH='/api/mapbox/token'


def cases()->list[Case]:
  out:list[Case]=[{'scenario':'empty-status','method':'GET','path':PATH+'s'}, {'scenario':'head-status','method':'HEAD','path':PATH+'s'}, {'scenario':'encoded-path-status','method':'GET','path':'/api/mapbox/%74okens'}]
  for seed in ('','a','abc','abcd','abcdefghijkl','abcdefghijklm','한글🚗abcdefghi🚘끝',' \tpk.fake_stored_token\n','\ud800'):
    out.append({'scenario':'mask-codepoint-boundary','method':'GET','path':PATH+'s','seed':seed})
  out.append({'scenario':'invalid-stored-utf8','method':'GET','path':PATH+'s','mode':'seed_invalid_utf8'})
  for body in (None,[],True,{}, {'key_type':'bad','token':PUBLIC},{'key_type':None,'token':PUBLIC},{'type':' SECRET ','value':SECRET},{'key_type':'secret','type':'public','token':SECRET,'value':PUBLIC}, {'token':'pk.short'}, {'token':' '+PUBLIC+' '}, {'token':'pk.'+'界'*17}, {'token':'pk.'+'x'*16}, {'token':'pk.'+'x'*17}, {'token':False}, {'token':{'a':1}}, {'token':'pk.'+'x'*15+'\ud800\ud801'}, {'token':PUBLIC}):
    out.append({'scenario':'set-format-alias-or-failure','method':'POST','path':PATH,'body':body})
  out.extend([{'scenario':'set-invalid-json','method':'POST','path':PATH,'raw':'{'}, {'scenario':'after-set-status','method':'GET','path':PATH+'s'}])
  for query in ('?type=secret','?key_type=','?key_type=invalid','?key_type=PUBLIC&type=secret'):
    out.append({'scenario':'clear-alias-default','method':'DELETE','path':PATH+query})
  out.extend([{'scenario':'set-public-recovery','method':'POST','path':PATH,'body':{'token':PUBLIC}}, {'scenario':'set-secret-recovery','method':'POST','path':PATH,'body':{'type':'secret','value':SECRET}}])
  for body in ({'key_type':'secret'},{'key_type':None},{'type':False},{'token':None},{'token':False},{'token':'bad'},{'token':'pk.'+'x'*17+'\u001c'},{'token':'pk.'+'x'*17+'\ninside'},{'key_type':'secret','token':SECRET+'\ud800'}, {'token':PUBLIC+'\ud800'}):
    out.append({'scenario':'validation-format-no-outbound','method':'POST','path':PATH+'/validate','body':body})
  out.extend([{'scenario':'validate-empty-body-stored','method':'POST','path':PATH+'/validate'}, {'scenario':'validate-invalid-json','method':'POST','path':PATH+'/validate','raw':'{'}, {'scenario':'validate-nonobject-default','method':'POST','path':PATH+'/validate','body':[]}, {'scenario':'validate-explicit-precedence','method':'POST','path':PATH+'/validate','body':{'token':'','value':PUBLIC}}])
  for status,response in ((200,{}),(200,{'code':'Ok'}),(200,{'code':'NoRoute','message':'fake denied'}),(200,{'code':None}),(200,{'code':0}),(200,{'code':'NoRoute','message':['fake']}),(200,None),(200,[]),(200,False),(401,{'message':'fake unauthorized'}),(403,{'code':'InvalidToken'}),(500,{}),(204,{})):
    out.append({'scenario':'public-online-json-status','method':'POST','path':PATH+'/validate','body':{'token':PUBLIC},'response_status':status,'response':response})
  for mode in ('empty','nonjson','invalid_utf8','gzip','br','cap_valid','cap_truncated','wrong_length','redirect_relative','redirect_uri','redirect_loop','redirect_chain_limit','slow_headers','slow_body','stall_headers','stall_body','no_status'):
    out.append({'scenario':'online-boundary-'+mode,'method':'POST','path':PATH+'/validate','body':{'token':PUBLIC},'mode':mode})
  out.extend([{'scenario':'query-percent-utf8','method':'POST','path':PATH+'/validate','body':{'token':'pk.~*+/@#=界0123456789'}}, {'scenario':'online-recovery','method':'POST','path':PATH+'/validate','body':{'token':PUBLIC}}, {'scenario':'final-status','method':'GET','path':PATH+'s'}])
  return out


async def main()->None:
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--binding',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--routing-only',action='store_true');args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
  load(args.binding.resolve(),f'ipc://{args.output.resolve()}/logs.sock',args.output/'binding-logs')
  features=ModuleType('openpilot.selfdrive.carrot.server.features');features.__path__=[str(Path('openpilot/selfdrive/carrot/server/features').resolve())];sys.modules[features.__name__]=features
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.features import mapbox_tokens as source
  from openpilot.selfdrive.carrot.server.services import params
  roots=[args.output/name for name in ('original-params','native-params')];stores=[Params(str(root.resolve())) for root in roots];params.Params=lambda:stores[0]
  outgoing=[];recipient_errors=[];active={'side':'source','index':-1,'case':{}}
  class Recipient(BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*args):pass
    def do_GET(self):
      side=active['side'];index=active['index'];case=active['case'];mode=case.get('mode','');status=case.get('response_status',200)
      outgoing.append({'side':side,'index':index,'path':self.path,'headers':{key.lower():value for key,value in self.headers.items()}})
      body=json.dumps(case.get('response',{'code':'Ok'})).encode();encoding=''
      if mode=='no_status':return
      if mode.startswith('redirect'):
        step=int(self.path.split('redirect-step-')[-1]) if 'redirect-step-' in self.path else 0
        if mode=='redirect_loop' or mode=='redirect_chain_limit' or step==0:
          self.send_response(302);self.send_header('URI' if mode=='redirect_uri' else 'Location','/redirect-step-1' if mode=='redirect_loop' else f'/redirect-step-{step+1}');self.send_header('Content-Length','0');self.end_headers();return
      if mode=='empty':body=b''
      if mode=='nonjson':body=b'fake <html>'
      if mode=='invalid_utf8':body=b'{"message":"\xff","code":"NoRoute"}'
      if mode=='gzip':body=gzip.compress(body);encoding='gzip'
      if mode=='br':body=brotli.compress(body);encoding='br'
      if mode=='cap_valid':body=b'{"code":"Ok"}'+b' '*6000
      if mode=='cap_truncated':body=b'{"code":"Ok","message":"'+b'x'*6000+b'"}'
      if mode=='slow_headers':
        self.wfile.write(b'HTTP/1.1 200 OK\r\n');self.wfile.flush();time.sleep(5);self.wfile.write(b'Content-Length: '+str(len(body)).encode()+b'\r\n');self.wfile.flush();time.sleep(5);self.wfile.write(b'\r\n'+body);self.wfile.flush();return
      if mode=='stall_headers':time.sleep(9)
      self.send_response(status)
      if encoding:self.send_header('Content-Encoding',encoding)
      self.send_header('Content-Length',str(len(body)+10 if mode=='wrong_length' else len(body)));self.send_header('Connection','close');self.end_headers()
      try:
        if mode=='stall_body':time.sleep(9)
        if mode=='slow_body':
          for part in (body[:4],body[4:8],body[8:]):self.wfile.write(part);self.wfile.flush();time.sleep(4)
        else:self.wfile.write(body)
      except (BrokenPipeError,ConnectionResetError) as error:recipient_errors.append({'side':side,'index':index,'error':type(error).__name__})
  recipient=ThreadingHTTPServer(('127.0.0.1',0),Recipient);actor=threading.Thread(target=recipient.serve_forever,daemon=True);actor.start();endpoint=f'http://127.0.0.1:{recipient.server_port}'
  original_open=urllib.request.urlopen
  def owned(request,timeout):return original_open(urllib.request.Request(endpoint+request.full_url.removeprefix('https://api.mapbox.com'),headers=dict(request.header_items())),timeout=timeout)
  source.urllib.request.urlopen=owned
  app=web.Application();source.register(app);runner=web.AppRunner(app);await runner.setup();site=web.TCPSite(runner,'127.0.0.1',0);await site.start();source_port=site._server.sockets[0].getsockname()[1]
  native=None;native_port=None;stderr=(args.output/'native.stderr').open('w')
  if args.binary:
    native=subprocess.Popen([str(args.binary.resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True);assert native.stdin is not None and native.stdout is not None
    init={'root':str(roots[1].resolve()),'state':str((args.output/'native-state').resolve()),'endpoint':endpoint};native.stdin.write(json.dumps(init)+'\n');native.stdin.flush();native_port=json.loads(native.stdout.readline())['port']
  def snapshot(store)->dict[str,Json]:return {name:Path(store.get_param_path(name)).read_bytes().hex() if Path(store.get_param_path(name)).is_file() else None for name in ('MapboxPublicKey','MapboxSecretKey')}
  failures=[];observations=[];inputs=[case for case in cases() if case['scenario']=='encoded-path-status'] if args.routing_only else cases()
  try:
    async with aiohttp.ClientSession() as client:
      for index,case in enumerate(inputs):
        if 'seed' in case or case.get('mode')=='seed_invalid_utf8':
          raw=b'\xff' if case.get('mode')=='seed_invalid_utf8' or case.get('seed')=='\ud800' else case['seed'].encode()
          for store in stores:Path(store.get_param_path('MapboxPublicKey')).write_bytes(raw)
        rows=[]
        for side,port,store in [('source',source_port,stores[0]),('native',native_port,stores[1])]:
          if port is None:continue
          active.update(side=side,index=index,case=case);before=len(outgoing);start=time.monotonic()
          data=case.get('raw',json.dumps(case['body']) if 'body' in case else '')
          async with client.request(case['method'],URL(f'http://127.0.0.1:{port}'+case['path'],encoded=True),data=data,allow_redirects=False) as response:
            body=await response.read();row={'status':response.status,'headers':{key.lower():value for key,value in response.headers.items() if key.lower() in ('content-type','content-length','allow')},'body_hex':body.hex(),'params':snapshot(store),'outbound':[dict(call,side='normalized') for call in outgoing[before:]]}
          observations.append({'side':side,'index':index,'scenario':case['scenario'],'elapsed_seconds':round(time.monotonic()-start,3),'output':row});rows.append(row)
        if len(rows)==2 and rows[0]!=rows[1]:failures.append({'index':index,'case':case,'original':rows[0],'native':rows[1]})
  finally:
    source.urllib.request.urlopen=original_open;await runner.cleanup();recipient.shutdown();recipient.server_close();actor.join();stderr.close()
    if native:
      assert native.stdin is not None;native.stdin.write('stop\n');native.stdin.flush();native.stdin.close();assert native.wait(timeout=10)==0
  (args.output/'inputs.json').write_text(json.dumps(inputs,indent=2)+'\n');(args.output/'observations.json').write_text(json.dumps(observations,indent=2)+'\n');(args.output/'outbound.json').write_text(json.dumps(outgoing,indent=2)+'\n');(args.output/'failures.json').write_text(json.dumps(failures,indent=2)+'\n');(args.output/'recipient-errors.json').write_text(json.dumps(recipient_errors,indent=2)+'\n')
  receipt={'cases':len(inputs),'passed':bool(args.binary) and not failures,'source_only':not bool(args.binary),'failures':len(failures),'comparison':'actual feature HTTP status/selectedheaders/rawbody/Params bytes plus real owned outbound path/query/headers'};(args.output/'result.json').write_text(json.dumps(receipt)+'\n');print(json.dumps(receipt));assert not failures


if __name__=='__main__':anyio.run(main,backend='asyncio')
