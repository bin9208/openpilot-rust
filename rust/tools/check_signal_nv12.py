"""Actual worker NV12 copy/conversion bytes with padding and separated UV plane."""
import argparse,ast,hashlib,json,subprocess
from pathlib import Path
from types import SimpleNamespace
import cv2
import numpy as np
from signal_source import source

def main():
  p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
  cv2.setNumThreads(1);cv2.ocl.setUseOpenCL(False)
  node=next(n for n in ast.parse(source('openpilot/selfdrive/modeld/signal_tracking_shadow.py')).body if isinstance(n,ast.FunctionDef) and n.name=='copy_nv12_rgb');scope={'REFERENCE_SIZE':(1344,760)};exec(compile(ast.Module(body=[node],type_ignores=[]),'e8e152db:copy_nv12_rgb','exec'),scope)
  cases=[];expected=bytearray();rng=np.random.default_rng(923)
  for i,stride in enumerate([1344,1408,1536,1344]):
    offset=stride*760+(0 if i%2 else 128);raw=rng.integers(0,256,offset+stride*380+32,np.uint8);path=a.output/f'{i}.nv12';path.write_bytes(raw.tobytes());case={'path':str(path.resolve()),'width':1344,'height':760,'stride':stride,'uv_offset':offset};cases.append(case)
    rgb=scope['copy_nv12_rgb'](SimpleNamespace(data=raw.tobytes(),**{k:v for k,v in case.items() if k!='path'}));expected.extend(rgb.tobytes())
  (a.output/'input.json').write_text(json.dumps(cases));(a.output/'expected.rgb').write_bytes(expected)
  if a.binary:
    actual=subprocess.check_output([str(a.binary.resolve())],input=json.dumps(cases).encode());(a.output/'actual.rgb').write_bytes(actual);assert actual==expected
    (a.output/'receipt.json').write_text(json.dumps({'bytes':len(actual),'sha256':hashlib.sha256(actual).hexdigest(),'byte_differences':0})+'\n')
  print(json.dumps({'frames':len(cases),'bytes':len(expected),'native_compared':bool(a.binary)}))
if __name__=='__main__':main()
