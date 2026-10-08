#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# How to run: from the repository root, with anyio/aiohttp supplied by the caller.
# python rust/tools/carrot_server_dashcam_media.py --binary PATH --output DIR [--real|--composed|--smoke]
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Final, TypedDict
from urllib.parse import quote

import anyio
from aiohttp import web
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media_source import application, source

STAMP: Final = 1700000000123456789
SELECTED: Final = ('content-type', 'content-length', 'cache-control', 'content-disposition', 'accept-ranges', 'content-range', 'allow')
WRAPPER: Final = '''#!/usr/bin/python3
import json,os,pathlib,subprocess,sys
root=pathlib.Path(__file__).parent
mode=(root/'mode').read_text().strip();args=sys.argv[1:];out=pathlib.Path(args[-1])
with (root/'commands.jsonl').open('a') as log:log.write(json.dumps({'mode':mode,'argv':args})+'\\n')
if mode=='invalid':os.write(1,b'\\xff');sys.exit(0)
if mode=='fail' or (mode=='retry' and args[args.index('-ss')+1]=='2') or (mode=='preview-fallback' and out.suffix=='.gif') or (mode=='silent' and '-an' not in args):out.write_bytes(b'partial');sys.exit(7)
if mode=='empty':out.write_bytes(b'');sys.exit(0)
if mode=='no-output':sys.exit(0)
if mode=='real':
 result=subprocess.run(['/usr/bin/ffmpeg',*args])
 if result.returncode:sys.exit(result.returncode)
else:out.write_bytes(('owned '+out.suffix).encode())
if out.exists():os.utime(out,ns=(1700000000123456789,1700000000123456789))
'''

class Capture(TypedDict):
    status: int
    headers: dict[str, str]
    body_base64: str

@dataclass(frozen=True, slots=True)
class Case:
    label: str
    kind: str
    mode: str
    segment: str
    method: str = 'GET'
    query: str = ''
    headers: tuple[tuple[str, str], ...] = ()
    clear: bool = True
    expected: int = 200

async def read_response(buffered: BufferedByteReceiveStream, method: str) -> Capture:
    raw = await buffered.receive_until(b'\r\n\r\n', 65536)
    lines = raw.decode('latin1').split('\r\n')
    status = int(lines[0].split()[1])
    parsed = {name.lower(): value.strip() for line in lines[1:] if ':' in line for name, value in [line.split(':', 1)]}
    length = int(parsed.get('content-length', '0')) if method != 'HEAD' else 0
    body = await buffered.receive_exactly(length) if length else b''
    return {'status': status, 'headers': parsed, 'body_base64': base64.b64encode(body).decode()}

async def capture(port: int, path: str, method: str, headers: tuple[tuple[str, str], ...]) -> Capture:
    async with await anyio.connect_tcp('127.0.0.1', port) as stream:
        fields = '\r\n'.join(f'{name}: {value}' for name, value in headers)
        await stream.send(f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{fields}\r\n\r\n'.encode())
        return await read_response(BufferedByteReceiveStream(stream),method)

async def persistent(port: int, path: str) -> dict:
    async with await anyio.connect_tcp('127.0.0.1',port) as stream:
        buffered = BufferedByteReceiveStream(stream)
        await stream.send(f'GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n'.encode())
        with anyio.fail_after(3):first = await read_response(buffered,'GET')
        eof = False
        with anyio.move_on_after(.2):
            try:await buffered.receive()
            except anyio.EndOfStream:eof = True
        followup = None
        try:
            await stream.send(b'GET /api/dashcam/video/owned--0 HTTP/1.1\r\nHost: localhost\r\n\r\n')
            with anyio.move_on_after(.3):followup = (await read_response(buffered,'GET'))['status']
        except (anyio.BrokenResourceError,anyio.ClosedResourceError,anyio.EndOfStream,anyio.IncompleteRead):followup = None
        first['headers'] = {key:value for key,value in first['headers'].items() if key not in ('date','server')}
        return {'first':first,'eof_after_response':eof,'followup_status':followup}

def save(path: Path, value) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=True)+'\n')

def command_rows(log: Path) -> list[dict]:
    return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

def normalized(value: Capture, conditional: bool) -> Capture:
    names = SELECTED + (('etag', 'last-modified') if conditional else ())
    return {'status': value['status'], 'headers': {key: item for key, item in value['headers'].items() if key in names}, 'body_base64': value['body_base64']}

async def main(binary: Path, output: Path, real: bool, composed: bool, smoke: bool) -> None:
    output.mkdir(parents=True, exist_ok=False)
    free = __import__('shutil').disk_usage(output).free
    save(output/'diskguard.json', {'free_bytes': free, 'reserve_bytes': 25*1024**3, 'estimated_growth_bytes': 16*1024**2})
    assert free >= 25*1024**3+16*1024**2
    root = output/'owned-root';root.mkdir()
    tools = output/'owned-bin';tools.mkdir()
    provider = tools/'ffmpeg';provider.write_text(WRAPPER);provider.chmod(0o700)
    mode = tools/'mode';mode.write_text('success');log = tools/'commands.jsonl'
    caches = [output/name for name in ('source-cache', 'native-cache')]
    segments = ('owned--0','short--1','aac--2','pcm--3','absent--4','empty--5','missing--6')
    for segment in segments:
        directory = root/segment;directory.mkdir()
        if segment not in ('absent--4','empty--5','missing--6'):
            (directory/('qcamera.ts' if segment in ('aac--2','pcm--3') else 'qcamera.mp4')).write_bytes(b'owned input')
        if segment=='empty--5':(directory/'qcamera.mp4').write_bytes(b'')
    (root/'missing--6').rmdir()
    generated = []
    if real:
        for segment, duration, audio in (('owned--0','4',None),('short--1','0.5',None),('aac--2','3','aac'),('pcm--3','3','pcm_bluray')):
            path = root/segment/('qcamera.ts' if audio else 'qcamera.mp4')
            args = ['/usr/bin/ffmpeg','-hide_banner','-loglevel','error','-y','-f','lavfi','-i','color=c=navy:s=160x120:r=10']
            if audio:args += ['-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-c:a',audio]
            args += ['-t',duration,'-c:v','mpeg2video' if audio else 'mpeg4']
            if audio=='pcm_bluray':args += ['-mpegts_m2ts_mode','1']
            args += [str(path)]
            result = subprocess.run(args,capture_output=True,timeout=20)
            (output/(segment+'-generation-stderr.txt')).write_bytes(result.stderr)
            generated.append({'command':args,'exit_code':result.returncode,'size':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
            assert result.returncode==0
    save(output/'synthetic-fixtures.json',generated)
    for path in root.glob('*/*'):os.utime(path,ns=(STAMP,STAMP))
    old_path = os.environ['PATH'];os.environ['PATH'] = str(tools)
    routes, _, proof = source(root,caches[0]);save(output/'source-proof.json',proof)
    runner = web.AppRunner(application(routes));await runner.setup()
    import socket
    listener = socket.socket();listener.bind(('127.0.0.1',0));listener.listen();listener.setblocking(False)
    source_port = listener.getsockname()[1];await web.SockSite(runner,listener).start()
    config = {'http':True,'composed':composed,'root':str(root),'cache':str(caches[1]),'ffmpeg':str(provider),'state':str(output/'owned-state')}
    state = output/'owned-state';state.mkdir();(state/'settings.json').write_text('{"params":[]}')
    native = await anyio.open_process([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    assert native.stdin is not None and native.stdout is not None and native.stderr is not None
    await native.stdin.send((json.dumps(config)+'\n').encode())
    stdout = BufferedByteReceiveStream(native.stdout)
    native_port = json.loads(await stdout.receive_until(b'\n',65536))['port']
    save(output/'invocation.json',{'command':[sys.executable,'-P',*sys.argv],'PYTHONPATH':os.environ.get('PYTHONPATH',''),'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'native_config':config})
    cases = []
    if real:
        cases = [Case('real-thumbnail','thumbnail','real','owned--0'),Case('real-short-thumbnail-second-seek','thumbnail','real','short--1'),Case('real-preview','preview','real','owned--0'),Case('real-aac-copy-remux','video','real','aac--2'),Case('real-pcm-silent-fallback','video','real','pcm--3'),Case('real-mp4-passthrough','video','real','owned--0')]
    elif composed:
        cases = [Case('app-thumbnail','thumbnail','retry','owned--0'),Case('app-preview-fallback','preview','preview-fallback','owned--0'),Case('app-ts-silent-remux','video','silent','aac--2'),Case('app-mp4-download-head','video','success','owned--0','HEAD','?download=1'),Case('app-absent-source','thumbnail','success','absent--4',expected=404),Case('app-missing-provider','preview','missing','owned--0',expected=503),Case('app-internal-utf8','video','invalid','aac--2',expected=500),Case('app-fresh-recovery','thumbnail','success','owned--0')]
    else:
        cases = [Case('thumbnail-first-seek','thumbnail','success','owned--0'),Case('thumbnail-second-seek','thumbnail','retry','owned--0'),Case('thumbnail-positive-cache','thumbnail','fail','owned--0',clear=False),Case('thumbnail-placeholder','thumbnail','fail','owned--0'),Case('thumbnail-empty-placeholder','thumbnail','empty','owned--0'),Case('thumbnail-no-output-placeholder','thumbnail','no-output','owned--0'),Case('preview-gif','preview','success','owned--0'),Case('preview-positive-cache','preview','fail','owned--0',clear=False),Case('preview-thumbnail-fallback','preview','preview-fallback','owned--0'),Case('preview-placeholder-fallback','preview','fail','owned--0'),Case('mp4-passthrough','video','fail','owned--0'),Case('ts-remux-audio-copy','video','success','aac--2'),Case('ts-positive-cache','video','fail','aac--2',clear=False),Case('ts-remux-silent-fallback','video','silent','aac--2'),Case('ts-last-resort','video','fail','aac--2'),Case('thumbnail-head','thumbnail','success','owned--0','HEAD'),Case('preview-head','preview','success','owned--0','HEAD'),Case('ts-download-head','video','fail','aac--2','HEAD','?download=1'),Case('mp4-download-raw-segment','video','fail',' owned--0 ','GET','?download=0&download='),Case('empty-download-query','video','fail','owned--0','GET','?download=&download=1'),Case('thumbnail-missing-provider','thumbnail','missing','owned--0',expected=503),Case('preview-missing-provider','preview','missing','owned--0',expected=503),Case('video-missing-provider','video','missing','aac--2',expected=503),Case('strict-utf8-failure','thumbnail','invalid','owned--0',expected=500),Case('missing-segment','video','success','missing--6',expected=404),Case('absent-video','preview','success','absent--4',expected=404),Case('zero-source','thumbnail','success','empty--5',expected=404),Case('invalid-segment','thumbnail','success','../owned--0',expected=400),Case('method-not-allowed','video','success','owned--0','POST',expected=405)]
    if smoke:cases = cases[:2]
    observations = [];commands = [];failures = [];codecs = [];sockets = []
    async def pair(case: Case, conditional: bool=False) -> None:
        start = len(command_rows(log));mode.write_text(case.mode)
        if case.clear:
            for cache in caches:
                for path in cache.glob('*/*'):
                    if path.is_file():path.unlink()
        if case.mode=='missing':provider.rename(tools/'disabled')
        request_path = '/api/dashcam/'+case.kind+'/'+quote(case.segment,safe='')+case.query
        with anyio.fail_after(20):
            expected = await capture(source_port,request_path,case.method,case.headers)
            actual = await capture(native_port,request_path,case.method,case.headers)
        if case.mode=='missing':(tools/'disabled').rename(provider)
        rows = command_rows(log)[start:]
        normalized_commands = [[{'mode':row['mode'],'argv':[v.replace(str(cache),'$CACHE') for v in row['argv']]} for row in rows if any(str(cache) in value for value in row['argv'])] for cache in caches]
        command_pair = {'scenario':case.label,'source':normalized_commands[0],'native':normalized_commands[1],'equal':normalized_commands[0]==normalized_commands[1]};commands.append(command_pair)
        row = {'scenario':case.label,'request':{'method':case.method,'path':request_path,'headers':case.headers},'source':expected,'native':actual,'equal':normalized(expected,conditional)==normalized(actual,conditional),'expected_status':case.expected,'status_correct':expected['status']==actual['status']==case.expected};observations.append(row)
        if not row['equal'] or not row['status_correct'] or not command_pair['equal']:failures.append(case.label)
        if not case.clear and rows:failures.append(case.label+' cache executed FFmpeg')
        if real and case.kind=='video':
            for cache in caches:
                path = cache/'video'/(hashlib.sha1(case.segment.encode()).hexdigest()[:24]+'.mp4') if case.segment!='owned--0' else root/'owned--0/qcamera.mp4'
                invocation = ['/usr/bin/ffprobe','-v','error','-show_entries','stream=codec_type,codec_name','-of','json',str(path)]
                probed = subprocess.run(invocation,capture_output=True,timeout=5)
                streams = json.loads(probed.stdout)['streams']
                audio = [item['codec_name'] for item in streams if item['codec_type']=='audio']
                wanted = ['aac'] if case.segment=='aac--2' else []
                codecs.append({'scenario':case.label,'command':invocation,'exit_code':probed.returncode,'streams':streams,'audio_matches':audio==wanted,'file_sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
                if probed.returncode or audio!=wanted:failures.append(case.label+' codec output')
    try:
        for case in cases:await pair(case)
        if composed and not smoke:
            mode.write_text('invalid')
            for cache in caches:
                for path in cache.glob('*/*'):
                    if path.is_file():path.unlink()
            for label,path in (('app-internal-utf8-closes','/api/dashcam/thumbnail/owned--0'),('app-passthrough-keepalive','/api/dashcam/video/owned--0')):
                expected = await persistent(source_port,path);actual = await persistent(native_port,path)
                row={'scenario':label,'source':expected,'native':actual,'equal':expected==actual};sockets.append(row)
                if expected!=actual:failures.append(label)
            assert sockets[0]['native']['first']['status']==500 and sockets[0]['native']['eof_after_response'] and sockets[0]['native']['followup_status'] is None
            assert sockets[1]['native']['first']['status']==200 and not sockets[1]['native']['eof_after_response'] and sockets[1]['native']['followup_status']==200
        if not real and not composed and not smoke:
            for cache in caches:
                path=cache/'thumb'/(hashlib.sha1(b'owned--0').hexdigest()[:24]+'.jpg');path.parent.mkdir(exist_ok=True);path.write_bytes(b'owned conditional JPEG');os.utime(path,ns=(STAMP,STAMP))
            await pair(Case('cached-jpeg-conditional-headers','thumbnail','fail','owned--0',clear=False),True)
            etag=observations[-1]['source']['headers']['etag']
            for label,method,headers,status in (('thumbnail-if-none-match','GET',(('If-None-Match',etag),),304),('thumbnail-range','GET',(('Range','bytes=1-4'),),206),('thumbnail-head-range','HEAD',(('Range','bytes=1-4'),),206)):
                await pair(Case(label,'thumbnail','fail','owned--0',method,headers=headers,clear=False,expected=status),True)
    finally:
        await native.stdin.send(b'\n');await native.stdin.aclose();await native.wait()
        remaining=b''
        try:
            while True:remaining+=await stdout.receive()
        except anyio.EndOfStream:pass
        errors=b''
        try:
            while True:errors+=await native.stderr.receive()
        except anyio.EndOfStream:pass
        (output/'native-stdout.txt').write_bytes(remaining);(output/'native-stderr.txt').write_bytes(errors)
        await runner.cleanup();os.environ['PATH']=old_path
    save(output/'command-pairs.json',commands);save(output/'observations.json',observations);save(output/'failures.json',failures)
    save(output/'codec-observations.json',codecs)
    save(output/'socket-observations.json',sockets)
    summary={'response_pairs':len(observations),'command_scenarios':len(commands),'argv_pairs':sum(len(row['source']) for row in commands),'failures':failures,'native_exit':native.returncode}
    save(output/'result.json',summary);print(json.dumps(summary,indent=2));assert not failures and native.returncode==0

if __name__=='__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--real',action='store_true')
    parser.add_argument('--composed',action='store_true')
    parser.add_argument('--smoke',action='store_true')
    args = parser.parse_args()
    anyio.run(main,args.binary.resolve(),args.output.resolve(),args.real,args.composed,args.smoke)
