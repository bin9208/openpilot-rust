import argparse
import copy
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
from can_source import ROOT
from check_card_vehicle import compare
import sys
import types
SOURCE_COMMIT = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
SOURCE_PATH = 'openpilot/selfdrive/carrot/xiaoge/xiaoge_vision.py'
SOURCE_BYTES = subprocess.check_output(['git', 'show', f'{SOURCE_COMMIT}:{SOURCE_PATH}'])
source_module = types.ModuleType('pinned_card_xiaoge_source')
sys.modules[source_module.__name__] = source_module
exec(compile(SOURCE_BYTES, f'{SOURCE_COMMIT}:{SOURCE_PATH}', 'exec'), source_module.__dict__)
apply_xiaoge_vision_result = source_module.apply_xiaoge_vision_result
parse_xiaoge_vision_payload = source_module.parse_xiaoge_vision_payload


def cases() -> list[dict]:
  base = dict(type='xiaogeVision', version=1, lane=dict(leftLine=0, rightLine=1, valid=True, receivedMonoTimeNanos=1),
              blindspot=dict(left=True, right=False, valid=True, receivedMonoTimeNanos=1))
  result = []
  def add(text: str | bytes, now=1, lanes=(20, 10), spots=(False, False)):
    payload = text.encode() if isinstance(text, str) else text
    result.append(dict(bytes=list(payload), now=now, lanes=lanes, spots=spots))
  for now, lanes, spots, detected in itertools.product((0,1,1_500_000_001,1_500_000_002,4_000_000_001,4_000_000_002),
          ((-1,0),(10,21),(32767,-32768)), ((False,False),(True,True)), ((-1,-1),(0,1),(1,0))):
    value = copy.deepcopy(base)
    value['lane']['leftLine'], value['lane']['rightLine'] = detected
    add(json.dumps(value), now, lanes, spots)
  for aggregate, left, right, now in itertools.product((0,1,2_000_000_001), (None,0,1,2_000_000_001,10**100), (None,0,1,2_000_000_001,10**100), (1,1_500_000_001,2_000_000_001)):
    value=copy.deepcopy(base); value['blindspot'].update(left=True,right=True,receivedMonoTimeNanos=aggregate)
    value['blindspot']['receivedMonoTimeNanosBySide']={key:stamp for key,stamp in [('left',left),('right',right)] if stamp is not None}
    add(json.dumps(value),now)
  for side_value in (None,True,False,0,1,[],{},'bad'):
    value=copy.deepcopy(base);value['blindspot']['receivedMonoTimeNanosBySide']=side_value;add(json.dumps(value))
    for side in ('left','right'):
      value=copy.deepcopy(base);value['blindspot']['receivedMonoTimeNanosBySide']={side:side_value};add(json.dumps(value))
    value=copy.deepcopy(base);value['blindspot']['validSides']=side_value;add(json.dumps(value))
  bad = [None, False, True, -2, 2, 1.0, '1', [], {}]
  for section, field in (('lane','leftLine'),('lane','rightLine'),('lane','valid'),('lane','receivedMonoTimeNanos'),
                         ('blindspot','left'),('blindspot','right'),('blindspot','valid'),('blindspot','receivedMonoTimeNanos')):
    for item in bad + [0,1,-1,2**64,10**100,-0.0]:
      value = copy.deepcopy(base); value[section][field] = item
      add(json.dumps(value))
    value=copy.deepcopy(base); del value[section][field]; add(json.dumps(value))
  for version in [None, False, True, 1, 1.0, 2, '1', [], {}, float('nan'), float('inf')]:
    value=copy.deepcopy(base); value['version']=version; add(json.dumps(value))
  texts=[json.dumps(base), json.dumps(base | dict(unused=float('nan'))), json.dumps(base | dict(unused=float('inf'))),
         json.dumps(base | dict(unused='\ud800')), json.dumps(base | dict(unused='\udc00')), json.dumps(base | dict(unused='\ud800\udc00')),
         json.dumps(base | dict(unused='\ud800')).replace('\\ud800','\ud800')]
  for text in texts:
    for encoding in ('utf-8','utf-8-sig','utf-16','utf-16-le','utf-16-be','utf-32','utf-32-le','utf-32-be'):
      add(text.encode(encoding, errors='surrogatepass'))
  for text in ('null','[]','true','{}','{','{"type":"xiaogeVision"}', '[NaN]', ''):
    add(text)
  for payload in (b'\x80',b'\xff',b'\xff\xfe\x00', b'\0\0\xfe\xff\0', b'\0\0\xfe\xff\0\x11\0\0'):
    add(payload)
  return result


def trace(case: dict) -> dict:
  state=SimpleNamespace(leftLaneLine=case['lanes'][0],rightLaneLine=case['lanes'][1],leftBlindspot=case['spots'][0],rightBlindspot=case['spots'][1])
  try:
    result=parse_xiaoge_vision_payload(bytes(case['bytes']))
  except (UnicodeDecodeError,ValueError,TypeError):
    accepted=False; result=None
  else:
    accepted=True
  applied=apply_xiaoge_vision_result(state,result,case['now'])
  return dict(accepted=accepted,applied=applied,lanes=[state.leftLaneLine,state.rightLaneLine],spots=[state.leftBlindspot,state.rightBlindspot])


def main() -> None:
  parser=argparse.ArgumentParser(); parser.add_argument('--binary',type=Path,required=True); parser.add_argument('--evidence',type=Path,required=True)
  parser.add_argument('--runner', action='append',default=[]); args=parser.parse_args(); args.evidence.mkdir(parents=True,exist_ok=True)
  request=cases(); expected=[trace(case) for case in request]
  (args.evidence/'input.json').write_text(json.dumps(request)+'\n'); (args.evidence/'source.json').write_text(json.dumps(expected)+'\n')
  target=args.evidence/'native.json'
  child=subprocess.run([*args.runner,args.binary.resolve(),target.resolve()],input=json.dumps(request),text=True,capture_output=True,check=False)
  (args.evidence/'process.log').write_text(child.stdout+child.stderr+f'\nEXIT {child.returncode}\n'); child.check_returncode()
  actual=json.loads(target.read_text())
  try: compare(expected,actual)
  except AssertionError as error:
    (args.evidence/'comparison-failure.txt').write_text(str(error)+'\n'); raise
  result=dict(status='pass',cases=len(request),runtime_python=False,observable='payload acceptance, exact lane/color merge, stock blindspot OR and freshness boundaries',
              runner=args.runner,binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),source_commit=SOURCE_COMMIT,source_sha256=hashlib.sha256(SOURCE_BYTES).hexdigest())
  (args.evidence/'result.json').write_text(json.dumps(result,indent=2)+'\n'); print(json.dumps(result))


if __name__=='__main__': main()
