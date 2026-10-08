# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import subprocess
from typing import Iterator, Literal, TypeAlias, TypedDict, assert_never
import zlib
import brotli

from original_params_binding import load

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']
Action: TypeAlias = Literal['init','backup','build','binary','parse','parse_binary']
class Case(TypedDict, total=False):
  action: Action
  values: dict[str,Json]
  version: int
  data: Json
  schema: dict[str,Json]
  absent_brotli: bool
  unavailable: bool
  root: str
  scenario: str


def corpus(params, backup:dict[str,Json]) -> Iterator[Case]:
  yield {'action':'backup','scenario':'real208-typed-backup'}
  for version in (2,3,4):
    built=getattr(params,f'_build_params_qr_payload_v{version}')(backup)
    yield {'action':'build','version':version,'scenario':f'real208-build-v{version}'}
    yield {'action':'parse','data':built['payload'],'scenario':f'real208-parse-v{version}'}
    yield {'action':'parse','data':built['payload'][:-12]+built['payload'][-12:].lower(),'scenario':f'checksum-case-v{version}'}
  yield {'action':'build','scenario':'real208-preferred3'}
  yield {'action':'build','absent_brotli':True,'scenario':'real208-absent-brotli4'}
  schema={'names':['Bool','Int','Text'],'types':{'Bool':1,'Int':2,'Text':0}}
  for value in ('','0','1','-0','01','-01','123','-19','١٢','１２','²','Ⅻ','14.999',False,True,14.5,15.5,None,{'한글':'é'},['a',2],'한글 🚗','\ud800'):
    for name in ('Unknown','Int','Bool'):
      yield {'action':'binary','values':{name:value},'schema':schema,'version':4,'scenario':f'value-{name}-{ascii(value)}'}
  for value in ('true','True','TRUE','on','yes',' true ',[],{}):
    yield {'action':'binary','values':{'Bool':value},'schema':schema,'version':3,'scenario':f'bool-exact-{ascii(value)}'}
  for values in ({'Unknown':'²'},{'Unknown':-(2**80)},{'한글':'surrogate\ud800'},{'\ud800':'a'},{'Unknown':{'z':1,'a':'é'}},{}):
    yield {'action':'build','values':values,'absent_brotli':True,'scenario':'preferred4-or2-fallback'}
  for value in (None,False,0,'',' ','wrong','[]',[],1,{'values':{'A':2},'type':'wrong'},{'values':2},' {"values":{"한글":"é"}} ','{broken'):
    yield {'action':'parse','data':value,'unavailable':True,'scenario':'direct-or-malformed'}
  names=params._backup_param_names(); code=params._param_short_code_bytes(names[0]);header=bytes([4,2])+b'xxxx'
  raws=[b'',b'\x04\x02',bytes([5,2])+b'xxxx',bytes([4,0])+b'xxxx',bytes([4,9])+b'xxxx',header,header+b'\x80',header+b'\x80'*10,
        header+b'\x01',header+b'\x01'+code,header+b'\x01'+code+b'\x09',header+b'\x01'+code+b'\x03\x80',header+b'\x01'+code+b'\x04\x05ab',
        header+b'\x01'+code+b'\x04\x01\xff\x00',header+b'\x00\x01\x05ab',header+b'\x00\x01\x01\xff\x00',header+b'\x00\x00tail',
        header+b'\x01zz\x02\x00',header+b'\x00\x01\x01A\x02',header+b'\x01'+code+b'\x03'+b'\x80'*9+b'\x01\x00',bytes([0,2])+b'xxxx\x00\x00']
  for index,raw in enumerate(raws):
    yield {'action':'parse_binary','data':list(raw),'scenario':f'binary-boundary-{index}'}
  def wrap(raw:bytes,version:int,compress:bool=True) -> str:
    encoded=zlib.compress(raw,9) if compress else raw
    checksum=hashlib.sha256(encoded).hexdigest()
    if version in (1,2):return f'CQR{version}.{params._b64url_encode(encoded)}.{checksum[:16 if version==1 else 12]}'
    return f'CQR4:{params._base45_encode(encoded)}:{checksum[:12].upper()}'
  for envelope in ({'type':'params_backup','version':1,'values':{'A':2}}, {'type':'wrong','values':{}},{'type':'params_backup','version':2,'values':{}},{'type':'params_backup','values':[]},[]):
    yield {'action':'parse','data':wrap(json.dumps(envelope).encode(),1),'unavailable':True,'scenario':'legacy1-envelope'}
  for envelope in ([2,[],{'A':'fallback'}],[4,[],None],[5,[]],[2],{'v':2,'d':[],'n':{}},[2,None],[2,[],1],[0,[],{'한글':'é'}],[2,[None,[],['unknown',4],['x',1,2]],{}],True):
    yield {'action':'parse','data':wrap(json.dumps(envelope).encode(),2),'scenario':'compact2-envelope'}
  for compressed in (b'',b'x',b'badheader',zlib.compress(header+b'\x00\x00')[:-1],zlib.compress(header+b'\x00\x00')+b'trailing',zlib.compress(header+b'\x00\x00')[:-1]+b'\x01'):
    yield {'action':'parse','data':wrap(compressed,4,False),'scenario':'zlib-consumption-or-error'}
  for payload in ('CQR4:','CQR4:X:1','CQR4:ZZZ:1','CQR4:::1','CQR4:??:1','CQR4:00:1','CQR2.A.1','CQR2.é.1','CQR2.!!.1','CQR2.AA=.1','CQR3:','CQR3:00:1'):
    yield {'action':'parse','data':payload,'scenario':'text-base-checksum-errors'}
  built=params._build_params_qr_payload_v3({'IsMetric':'1'})['payload']
  yield {'action':'parse','data':built,'absent_brotli':True,'scenario':'brotli-absent-parse-order'}
  import brotli
  for compressed in (b'',b'bad',brotli.compress(header+b'\x00\x00')[:-1],brotli.compress(header+b'\x00\x00')+b'x'):
    payload=f'CQR3:{params._base45_encode(compressed)}:{hashlib.sha256(compressed).hexdigest()[:12].upper()}'
    yield {'action':'parse','data':payload,'scenario':'brotli-stream-errors'}
  for size in (2,3):
    seen={};pair=[]
    for index in range(20000):
      name=f'Collision{index}';code=hashlib.sha256(name.encode()).digest()[:size]
      if code in seen:pair=[seen[code],name];break
      seen[code]=name
    assert pair,'bounded collision fixture'
    collision={'names':pair,'types':{}};values={pair[0]:'123',pair[1]:'한글'}
    yield {'action':'build','values':values,'schema':collision,'version':2 if size==3 else 4,'scenario':f'collision-fallback-size{size}'}
    with_schema=params._backup_param_names;params._backup_param_names=lambda:pair
    try:payload=getattr(params,f'_build_params_qr_payload_v{2 if size==3 else 4}')(values)['payload']
    finally:params._backup_param_names=with_schema
    yield {'action':'parse','data':payload,'schema':collision,'scenario':f'collision-roundtrip-size{size}'}
  for version in (2,3,4):
    yield {'action':'build','values':{'Unknown':'12','한글':'é'},'unavailable':True,'version':version,'scenario':'provided-build-unavailable-params'}
  yield {'action':'parse','data':wrap(b'[2,[]]',2),'unavailable':True,'scenario':'mapped-parse-unavailable-params'}


def main() -> None:
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--binding',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
  load(args.binding.resolve(),f'ipc://{args.output.resolve()}/logs.sock',args.output/'binding-logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import params
  roots=[args.output/name for name in ('original-params','native-params')]
  stores=[Params(str(root.resolve())) for root in roots]
  for store in stores:
    for name,value in {'IsMetric':b'0','CarName':'한글 🚗'.encode(),'CruiseGapLevels':b'4','InstallDate':b'2026-10-08T00:00:00'}.items():Path(store.get_param_path(name)).write_bytes(value)
  params.Params=lambda:stores[0]
  backup=params.get_all_param_values_for_backup();names=params._backup_param_names();types=params._backup_param_type_map(names)
  (args.output/'schema.json').write_text(json.dumps({'values':backup,'names':names,'types':{name:int(kind) for name,kind in types.items()}})+'\n')
  original_names=params._backup_param_names;original_types=params._backup_param_type_map;original_brotli=params._load_brotli_module
  @contextmanager
  def ports(case:Case):
    saved=params.HAS_PARAMS
    if case.get('unavailable'):params.HAS_PARAMS=False
    if 'schema' in case:
      params._backup_param_names=lambda:case['schema']['names']
      params._backup_param_type_map=lambda names=None:{key:params.ParamKeyType(kind) for key,kind in case['schema']['types'].items()}
    if case.get('absent_brotli'):
      def missing():raise ModuleNotFoundError("No module named 'brotli'")
      params._load_brotli_module=missing
    try:yield
    finally:params.HAS_PARAMS=saved;params._backup_param_names=original_names;params._backup_param_type_map=original_types;params._load_brotli_module=original_brotli
  def source(case:Case)->Json:
    with ports(case):
      match case['action']:
        case 'init':return None
        case 'backup':return params.get_all_param_values_for_backup()
        case 'build':
          values=case.get('values',backup)
          return getattr(params,f'_build_params_qr_payload_v{case["version"]}')(values) if 'version' in case else params.build_params_qr_payload(values)
        case 'binary':return list(params._build_params_qr_binary(case['values'],case['version']))
        case 'parse':return params.parse_params_qr_payload(case['data'])
        case 'parse_binary':return params._parse_params_qr_binary(bytes(case['data']))
        case unknown:assert_never(unknown)
  inputs=[{'action':'init','root':str(roots[1].resolve()),'scenario':'owned-native-params'},*corpus(params,backup)]
  failures=[]
  with (args.output/'inputs.jsonl').open('w') as requests,(args.output/'original.jsonl').open('w') as expected,(args.output/'native.jsonl').open('w') as actual,(args.output/'native.stderr').open('w') as stderr:
    process=subprocess.Popen([str(args.binary.resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True) if args.binary else None
    try:
      for index,case in enumerate(inputs):
        try:wanted=source(case)
        except (ValueError,TypeError,RuntimeError,OSError,AttributeError,OverflowError,ImportError,zlib.error,brotli.error) as error:wanted={'error':str(error)}
        request=json.dumps(case);requests.write(request+'\n');expected.write(json.dumps(wanted)+'\n');expected.flush()
        if process:
          assert process.stdin is not None and process.stdout is not None
          process.stdin.write(request+'\n');process.stdin.flush();got=json.loads(process.stdout.readline());actual.write(json.dumps(got)+'\n');actual.flush()
          if got!=wanted:failures.append({'index':index,'case':case,'original':wanted,'native':got})
    finally:
      if process:
        assert process.stdin is not None
        process.stdin.close();assert process.wait(timeout=10)==0
  (args.output/'failures.json').write_text(json.dumps(failures)+'\n')
  receipt={'cases':len(inputs),'backup_keys':len(backup),'passed':bool(args.binary) and not failures,'source_only':not bool(args.binary),'failures':len(failures),'comparison':'exact QR payload bytes/metadata, raw binary tags, returned objects and original error text'}
  (args.output/'result.json').write_text(json.dumps(receipt)+'\n');print(json.dumps(receipt));assert not failures


if __name__=='__main__':main()
