#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run from the repository root with source dependencies supplied by the caller.
# python rust/tools/carrot_server_dashcam_upload.py --binary PATH --worker PATH --output NEW_DIR
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from urllib.parse import quote

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from check_dashcam_runtime import Receiver, normalize
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_upload_peer import HeldReceiver

def save(path: Path, value) -> None:
    path.write_text(json.dumps(value, indent=2)+'\n')

def children(pid: int) -> list[dict]:
    result = []
    for task in Path(f'/proc/{pid}/task').glob('*'):
        try:
            ids = (task/'children').read_text().split()
            for child in ids:
                stat = Path(f'/proc/{child}/stat').read_text()
                result.append({'pid': int(child), 'owner_tid': int(task.name), 'starttime':stat.rsplit(')',1)[1].split()[19], 'stat': stat, 'executable': os.readlink(f'/proc/{child}/exe')})
        except FileNotFoundError:
            continue
    return result

def alive(child: dict) -> bool:
    try:
        return Path('/proc/'+str(child['pid'])+'/stat').read_text().rsplit(')',1)[1].split()[19]==child['starttime']
    except FileNotFoundError:
        return False

async def request(port: int, path: str, method: str='GET', body: bytes=b'', headers: tuple=()) -> dict:
    async with await anyio.connect_tcp('127.0.0.1',port) as stream:
        fields = '\r\n'.join([f'{method} {path} HTTP/1.1','Host: localhost','Connection: close',f'Content-Length: {len(body)}',*[f'{name}: {value}' for name,value in headers]])
        await stream.send((fields+'\r\n\r\n').encode()+body)
        with anyio.fail_after(5):
            result = await read_response(BufferedByteReceiveStream(stream),method)
        raw = base64.b64decode(result['body_base64'])
        result['payload'] = json.loads(raw) if raw and result['headers'].get('content-type','').startswith('application/json') else raw.decode('utf8')
        return result

class Fixture:
    def __init__(self,binary: Path,worker: Path,output: Path,scenario: str,composed: bool=False):
        self.binary=binary;self.worker=worker;self.output=output;self.output.mkdir()
        self.receivers=[HeldReceiver(),HeldReceiver()] if scenario=='cancel' else [Receiver(scenario),Receiver(scenario)];self.processes=[];self.logs=[];self.ports=[];self.rows=[];self.failures=[]
        self.root=output/'owned-root';self.root.mkdir()
        self.segments=['00000001--1234567890--'+str(index) for index in range(3)]
        for index,segment in enumerate(self.segments):
            directory=self.root/segment;directory.mkdir();(directory/'rlog.zst').write_bytes(bytes([index+10])*1024);(directory/'qcamera.ts').write_bytes(bytes([index+20])*4096)
        (self.root/self.segments[2]/'rlog.lock').write_bytes(b'owned lock')
        self.ids=[];self.accepted_workers=[];self.composed=composed
    async def start(self) -> None:
        for side,receiver in enumerate(self.receivers):
            settings={'root':str(self.root),'base_url':receiver.base,'token':'synthetic-session','metadata':{'carName':'fixture car','dongleId':'test-device','serial':'test-serial','branch':'dev','commit':'1234','commitDate':'2026-09-30'},'webhook':receiver.base+'/webhook','concurrency':1}
            state=self.output/('source-state' if side==0 else 'native-state');state.mkdir();(state/'settings.json').write_text('{"params":[]}')
            config={'root':str(self.root),'settings':settings,'output':str(self.output),'worker':str(self.worker),'composed':self.composed,'state':str(state)}
            command=[sys.executable,'-P',str(Path(__file__).with_name('carrot_server_dashcam_upload_source.py'))] if side==0 else [str(self.binary)]
            log=(self.output/('source-process.log' if side==0 else 'native-process.log')).open('wb');self.logs.append(log)
            environment=os.environ.copy();environment['OPENPILOT_PREFIX']='owned-dashcam-'+str(os.getpid())+'-'+str(side)
            process=await anyio.open_process(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,env=environment)
            self.processes.append(process)
            await process.stdin.send((json.dumps(config)+'\n').encode());reader=BufferedByteReceiveStream(process.stdout)
            with anyio.fail_after(5):ready=json.loads(await reader.receive_until(b'\n',65536))
            self.ports.append(ready['port'])
        save(self.output/'invocation.json',{'command':[sys.executable,'-P',*sys.argv],'binary':str(self.binary),'binary_sha256':hashlib.sha256(self.binary.read_bytes()).hexdigest(),'worker':str(self.worker),'worker_exists':self.worker.is_file(),'worker_sha256':hashlib.sha256(self.worker.read_bytes()).hexdigest() if self.worker.is_file() else None,'root':str(self.root),'recipients':[receiver.base for receiver in self.receivers]})
    def normalized(self,response: dict,side: int,live: bool=False,head: bool=False) -> dict:
        payload=normalize(response['payload'],self.receivers[side].base)
        if live and isinstance(payload,dict):
            state=payload.get('job',payload)
            if state.get('status')=='running':
                for key in ('progress','phase_current','bytes_current'):state.pop(key,None)
            if 'log' in state:state['log']='\n'.join(sorted(state['log'].splitlines()))
        selected=('content-type','allow') if live else ('content-type','content-length','allow')
        return {'status':response['status'],'headers':{key:value for key,value in response['headers'].items() if key in selected and not (head and key=='content-length')},'payload':payload}
    async def pair(self,label: str,path: str,method: str='GET',body: bytes=b'',status: int=200,live: bool=False,headers: tuple=()) -> list[dict]:
        responses=[]
        for side,port in enumerate(self.ports):
            owned_path=path.replace('$ID',quote(self.ids[side],safe='')) if '$ID' in path else path
            owned_body=body.replace(b'$ID',self.ids[side].encode()) if b'$ID' in body else body
            responses.append(await request(port,owned_path,method,owned_body,headers))
        equal=self.normalized(responses[0],0,live,method=='HEAD')==self.normalized(responses[1],1,live,method=='HEAD')
        row={'scenario':label,'request':{'path':path,'method':method,'body_base64':base64.b64encode(body).decode(),'headers':headers},'source':responses[0],'native':responses[1],'equal':equal,'expected_status':status};self.rows.append(row)
        if not equal or any(response['status']!=status for response in responses):self.failures.append(label)
        return responses
    async def begin(self,segment: str) -> None:
        responses=await self.pair('accepted-job','/api/dashcam/upload/start','POST',json.dumps({'segment':segment}).encode(),live=True)
        self.ids=[response['payload']['job_id'] for response in responses]
        assert all(response['payload']['ok'] and response['payload']['status']=='running' for response in responses)
        for receiver in self.receivers:assert await anyio.to_thread.run_sync(receiver.started.wait,5)
    async def terminal(self) -> None:
        with anyio.fail_after(15):
            while True:
                snapshots=[await request(port,'/api/dashcam/upload/job?id='+job_id) for port,job_id in zip(self.ports,self.ids)]
                if all(snapshot['payload']['done'] for snapshot in snapshots):break
                await anyio.sleep(.01)
        await self.pair('terminal-job','/api/dashcam/upload/job?id=$ID',live=True)
    async def close(self) -> None:
        before=children(self.processes[1].pid) if len(self.processes)>1 else []
        errors=[]
        for process in self.processes:
            try:
                if process.returncode is None:
                    await process.stdin.send(b'\n');await process.stdin.aclose()
                with anyio.fail_after(6):await process.wait()
            except (anyio.BrokenResourceError,anyio.ClosedResourceError,BrokenPipeError,TimeoutError) as error:
                errors.append(f'process {process.pid}: {error}')
                if process.returncode is None:
                    try:
                        process.kill()
                        with anyio.fail_after(3):await process.wait()
                    except (ProcessLookupError,TimeoutError) as cleanup_error:errors.append(str(cleanup_error))
            finally:
                if process.returncode is not None:
                    try:await process.aclose()
                    except (anyio.BrokenResourceError,anyio.ClosedResourceError,OSError) as cleanup_error:errors.append(str(cleanup_error))
        if self.receivers[0].scenario=='stale':
            for receiver in self.receivers:
                if not await anyio.to_thread.run_sync(receiver.disconnected.wait,2):errors.append('held recipient did not disconnect')
        for log in self.logs:
            try:log.close()
            except OSError as error:errors.append(str(error))
        tracked={(child['pid'],child['starttime']):child for child in self.accepted_workers+before}
        after=[{**child,'alive':alive(child)} for child in tracked.values()]
        if any(child['alive'] for child in after):errors.append('owned worker still alive after server stop')
        save(self.output/'worker-ownership.json',{'before_server_stop':before,'after_server_stop':after,'server_exit_codes':[process.returncode for process in self.processes],'recipient_disconnects':[receiver.disconnected.is_set() for receiver in self.receivers],'cleanup_errors':errors})
        for receiver in self.receivers:
            save(self.output/('source-receiver.json' if receiver is self.receivers[0] else 'native-receiver.json'),receiver.captures)
            try:await anyio.to_thread.run_sync(receiver.close)
            except OSError as error:errors.append(str(error))
        save(self.output/'observations.json',self.rows);save(self.output/'failures.json',self.failures)
        assert not errors and all(process.returncode==0 for process in self.processes)

async def boundary(fixture: Fixture) -> None:
    for operation in ('summary','start'):
        path='/api/dashcam/upload/'+operation
        for label,body,status in [('empty',b'',400),('malformed',b'{',400),('null',b'null',500),('list',b'[]',500),('false-members',b'{"segments":[null,false,0,""]}',400),('bad-member',b'{"segments":[true]}',400),('missing',b'{"segment":"missing--0"}',409),('locked',json.dumps({'segment':fixture.segments[2]}).encode(),409)]:
            await fixture.pair(operation+'-'+label,path,'POST',body,status)
    for body in ({'segment':fixture.segments[0]},{'segments':[fixture.segments[1],None,False,fixture.segments[0],fixture.segments[0]]},{'segments':'fallback','segment':' '+fixture.segments[0]+' '}):
        await fixture.pair('summary-valid-selection','/api/dashcam/upload/summary','POST',json.dumps(body).encode())
    for query,status in [('',400),('?id=+',400),('?id=missing',404),('?id=&id=missing',400)]:await fixture.pair('job-query'+query,'/api/dashcam/upload/job'+query,status=status)
    await fixture.pair('job-head-missing','/api/dashcam/upload/job?id=missing','HEAD',status=404)
    for label,body,status in [('empty',b'',400),('malformed',b'{',400),('false',b'{"id":false}',400),('coerced',b'{"id":true}',404),('alias',b'{"id":"","job_id":" missing "}',404),('list',b'[]',500)]:await fixture.pair('cancel-'+label,'/api/dashcam/upload/cancel','POST',body,status)
    await fixture.pair('summary-get-405','/api/dashcam/upload/summary',status=405)
    await fixture.pair('job-post-405','/api/dashcam/upload/job','POST',status=405)

async def lifecycle(fixture: Fixture,shutdown: bool) -> None:
    await fixture.begin(fixture.segments[0])
    before=children(fixture.processes[1].pid);assert len(before)==1
    fixture.accepted_workers=before
    await fixture.pair('running-job-across-request','/api/dashcam/upload/job?id=$ID',live=True)
    await fixture.pair('running-job-head','/api/dashcam/upload/job?id=$ID','HEAD',live=True)
    await fixture.pair('duplicate-start-409','/api/dashcam/upload/start','POST',json.dumps({'segment':fixture.segments[0]}).encode(),409,True)
    after=children(fixture.processes[1].pid)
    identity=lambda rows:[(child['pid'],child['owner_tid'],child['starttime']) for child in rows]
    assert identity(before)==identity(after)
    save(fixture.output/'accepted-owner-across-requests.json',{'before':before,'after':after,'same_worker_and_owner_tid':identity(before)==identity(after)})
    if not shutdown:
        await fixture.pair('cancel-running','/api/dashcam/upload/cancel','POST',b'{"job_id":"$ID"}',live=True)
        for receiver in fixture.receivers:receiver.release.set()
        await fixture.terminal()
        await fixture.pair('cancel-already-done','/api/dashcam/upload/cancel','POST',b'{"id":"$ID"}',live=True)

async def composed(fixture: Fixture) -> None:
    await fixture.pair('app-summary','/api/dashcam/upload/summary','POST',json.dumps({'segment':fixture.segments[0]}).encode())
    await fixture.pair('app-job-missing','/api/dashcam/upload/job?id=missing','HEAD',status=404)
    await fixture.pair('app-start-incomplete','/api/dashcam/upload/start','POST',json.dumps({'segment':fixture.segments[2]}).encode(),409)
    await fixture.begin(fixture.segments[0])
    fixture.accepted_workers=children(fixture.processes[1].pid)
    await fixture.pair('app-job-running','/api/dashcam/upload/job?id=$ID',live=True)
    await fixture.pair('app-cancel','/api/dashcam/upload/cancel','POST',b'{"id":"$ID"}',live=True)
    for receiver in fixture.receivers:receiver.release.set()
    await fixture.terminal()

async def packaging(fixture: Fixture) -> None:
    assert fixture.worker==fixture.binary.parent/'openpilot-dashcam-upload' and not fixture.worker.exists()
    port=fixture.ports[1]
    before=await request(port,'/api/params_bulk?names=OwnedIsolationProbe')
    failed=await request(port,'/api/dashcam/upload/start','POST',json.dumps({'segment':fixture.segments[0]}).encode())
    after=await request(port,'/api/params_bulk?names=OwnedIsolationProbe')
    summary=await request(port,'/api/dashcam/upload/summary','POST',json.dumps({'segment':fixture.segments[0]}).encode())
    assert before['status']==after['status']==summary['status']==200 and failed['status']==500
    assert before['payload']['ok'] and after['payload']['ok'] and summary['payload']['ok']
    assert not failed['payload']['ok'] and 'No such file or directory' in failed['payload']['error']
    assert not children(fixture.processes[1].pid) and not fixture.receivers[1].captures
    save(fixture.output/'packaging-observations.json',{'expected_missing_sibling':str(fixture.worker),'app_unrelated_before':before,'upload_start':failed,'app_unrelated_after':after,'summary_without_worker':summary,'no_worker_process':True,'no_upload_requests':True,'classification':'native packaging dependency failure; source async transfer failure is not claimed equivalent'})

async def success(fixture: Fixture) -> None:
    await fixture.begin(fixture.segments[0])
    fixture.accepted_workers=children(fixture.processes[1].pid)
    await fixture.terminal()
    terminal=fixture.rows[-1]
    for side in ('source','native'):
        state=terminal[side]['payload']
        assert state['status']=='done' and state['done'] and state['phase']=='complete'
        assert state['error'] is None and state['progress']==100 and state['result']['ok']
        assert state['bytes_current']==state['bytes_total']==5120
    uploads=[]
    for receiver in fixture.receivers:
        captured=sorted((row['path'].replace(receiver.base,'http://fixture'),row['size'],row['sha256'],row['auth']) for row in receiver.captures if row['method']=='PUT')
        uploads.append(captured)
    assert uploads[0]==uploads[1] and len(uploads[0])==2
    expected=sorted((name,(fixture.root/fixture.segments[0]/name).stat().st_size,hashlib.sha256((fixture.root/fixture.segments[0]/name).read_bytes()).hexdigest()) for name in ('qcamera.ts','rlog.zst'))
    assert sorted((row[0].rsplit('/',1)[-1],row[1],row[2]) for row in uploads[0])==expected
    save(fixture.output/'success-observations.json',{'terminal_source':terminal['source']['payload'],'terminal_native':terminal['native']['payload'],'source_uploaded_bytes':uploads[0],'native_uploaded_bytes':uploads[1],'source_file_hashes':expected,'deterministic_result_error_progress_fields_compared':True,'raw_complete_results_captured':True})

async def main() -> None:
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--worker',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);selection=parser.add_mutually_exclusive_group();selection.add_argument('--boundary-only',action='store_true');selection.add_argument('--lifecycle-only',action='store_true');selection.add_argument('--composed-only',action='store_true');selection.add_argument('--packaging-only',action='store_true');selection.add_argument('--success-only',action='store_true');args=parser.parse_args()
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False);binary=args.binary.resolve();worker=args.worker.resolve()
    free=shutil.disk_usage(output).free;save(output/'diskguard.json',{'free_bytes':free,'reserve_bytes':25*1024**3,'estimated_growth_bytes':16*1024**2});assert free>=25*1024**3+16*1024**2
    groups=([] if args.lifecycle_only else [('boundary','normal')])+([] if args.boundary_only else [('cancel','cancel'),('server-stop','stale')]);rows=[];failures=[]
    if args.composed_only:groups=[('application','cancel')]
    if args.packaging_only:groups=[('packaging','normal')]
    if args.success_only:groups=[('success','normal')]
    for name,scenario in groups:
        fixture=Fixture(binary,worker,output/name,scenario,args.composed_only or args.packaging_only or args.success_only)
        try:
            await fixture.start()
            if name=='success':await success(fixture)
            elif name=='packaging':await packaging(fixture)
            elif name=='application':await composed(fixture)
            elif name=='boundary':await boundary(fixture)
            else:await lifecycle(fixture,name=='server-stop')
        finally:await fixture.close()
        rows+=fixture.rows;failures+=fixture.failures
    save(output/'result.json',{'response_pairs':len(rows),'failures':failures,'surface':'original active upload route/helper bodies and real native Manager/owned worker; all recipients loopback','reused':'unchanged Manager state/packet/transport corpus; independent-clock IDs/times/rates/revisions normalized by existing check_dashcam_runtime helper; only in-flight counters normalized, terminal progress/counters preserved; log line ordering normalized explicitly'})
    print(json.dumps({'response_pairs':len(rows),'failures':failures},indent=2));assert not failures

if __name__=='__main__':anyio.run(main)
