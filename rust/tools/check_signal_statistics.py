"""Native-equivalence oracle for final-source small image percentile optimization."""
import argparse
import ast
import json
from pathlib import Path
import subprocess
import math
import numpy as np
from signal_source import source,compare


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  node=next(n for n in ast.parse(source('tools/signal_analysis/signal_tracker.py')).body if isinstance(n,ast.FunctionDef) and n.name=='small_percentile')
  scope={'np':np,'math':math};exec(compile(ast.Module(body=[node],type_ignores=[]),'e8e152db:small_percentile','exec'),scope)
  rng=np.random.default_rng(7123);cases=[];expected=[]
  for n in [1,2,3,4,5,6,10,31,64,128,1024]:
    for percent in [0.,10.,25.,50.,70.,80.,85.,90.,100.]:
      for kind in range(3):
        values=rng.integers(0,256,n).astype(np.float64)
        if kind==1:values/=255.
        if kind==2:values-=128.
        cases.append({'values':values.tolist(),'percent':percent})
        expected.append({'percentile':scope['small_percentile'](values,percent),'median':float(np.median(values))})
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw)
    (args.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'cases':len(cases),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
