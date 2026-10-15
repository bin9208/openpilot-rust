"""Final-source ISO-BMFF parser differential, including malformed bounded boxes."""
import argparse
import ast
import datetime
import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess
import time
from typing import Any

PIN='e8e152dbcfd9574a1791ed12026d1880e87689b3'
PATH='openpilot/selfdrive/carrot/server/features/screenrecord/catalog.py'
def box(kind,payload,style=0):
  return (len(payload)+8).to_bytes(4,'big')+kind+payload if style==0 else (b'\0\0\0\1'+kind+(len(payload)+16).to_bytes(8,'big')+payload if style==1 else b'\0\0\0\0'+kind+payload)
def timescale(scale,duration,version):
  return bytes([version])+b'\0'*3+b'\0'*(16 if version==1 else 8)+scale.to_bytes(4,'big')+duration.to_bytes(8 if version==1 else 4,'big')
def main():
  p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('output',type=Path);p.add_argument('--runner',nargs=argparse.REMAINDER,default=[]);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
  source=subprocess.check_output(['git','show',PIN+':'+PATH]);tree=ast.parse(source)
  scope={'Any':Any,'os':os,'re':re,'datetime':datetime.datetime,'date_label':lambda stamp:datetime.datetime.fromtimestamp(stamp).strftime('%Y-%m-%d %H:%M')}
  selected=[node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name in ['_name_start_epoch','_read_box','_iter_boxes','_find_path','_read_timescale_duration','_parse_video_info']]
  exec(compile(ast.Module(body=selected,type_ignores=[]),PATH,'exec'),scope)
  rng=random.Random(338);rows=[]
  for index in range(240):
    version=index%2;scale=rng.choice([0,1,24,1000,90000]);duration=rng.choice([0,1,12345,2**32-1]);frames=rng.randrange(10000)
    header=b'\0'*8+(b'vide' if index%7 else b'soun');samples=b'\0'*8+frames.to_bytes(4,'big');dimensions=b'\0'*40+rng.randrange(4096).to_bytes(2,'big')+rng.randrange(4096).to_bytes(2,'big')
    track=box(b'trak',box(b'mdia',box(b'hdlr',header)+box(b'mdhd',timescale(scale,duration,version))+box(b'minf',box(b'stbl',box(b'stsz',samples)+box(b'stsd',dimensions)))));
    payload=box(b'moov',box(b'mvhd',timescale(1000,19000,version))+track,index%3)
    if index%5==0:payload=payload[:rng.randrange(len(payload)+1)]
    if index%13==0:payload=rng.randbytes(index)
    path=a.output/f'20261015-010203-{index}.mp4';path.write_bytes(payload);os.utime(path,ns=(-500000000,-500000000) if index%17==0 else (1700000000123456789,1700000000123456789))
    expected=scope['_parse_video_info'](str(path),path.stat())
    request={'operation':'info','path':str(path.resolve())}
    run=subprocess.run([*a.runner,str(a.binary.resolve())],input=json.dumps(request)+'\n',text=True,capture_output=True,check=True)
    native=json.loads(run.stdout)['value'];assert expected==native,(index,expected,native)
    rows.append({'index':index,'sha256':hashlib.sha256(payload).hexdigest(),'info':native})
  for zone in ['UTC','Asia/Seoul','America/New_York']:
    os.environ['TZ']=zone;time.tzset()
    for name in ['20261015-010203.mp4','20260230-240000','20260308-023000.mp4','20261101-013000.mp4','20261015-010203extra','not-a-date','٢٠٢٦1015-010203.mp4']+[('20261015-010203'[:i]+chr(0x660+int('20261015-010203'[i]))+'20261015-010203'[i+1:]) for i in range(15) if i!=8]:
      expected=scope['_name_start_epoch'](name)
      run=subprocess.run([*a.runner,str(a.binary.resolve())],input=json.dumps({'operation':'start_epoch','name':name})+'\n',text=True,capture_output=True,check=True)
      native=json.loads(run.stdout)['value'];assert expected==native,(zone,name,expected,native)
  (a.output/'result.json').write_text(json.dumps({'source_commit':PIN,'source_sha256':hashlib.sha256(source).hexdigest(),'file_cases':len(rows),'timestamp_cases':63,'rows':rows},indent=2)+'\n');print('PASS240source MP4 records and63local timestamp cases')
if __name__=='__main__':main()
