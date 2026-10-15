"""Original final run_selected prerequisite with an inert execution leaf."""
import argparse,itertools,json
from pathlib import Path
from signal_source import classes

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
 args.output.parent.mkdir(parents=True,exist_ok=True)
 cases=[]
 for mask in range(16):
  cases.append(dict(files=[bool(mask&(1<<index)) for index in range(4)],fault=None))
 for index,kind in itertools.product(range(4),('permission','io')):
  cases.append(dict(files=[True]*4,fault=dict(index=index,kind=kind)))
 expected=[]
 for case in cases:
  trace=[]
  class FakePath:
   def __init__(self,value):self.value=str(value)
   def __truediv__(self,value):return FakePath(self.value+'/'+str(value))
   def is_file(self):
    index=len(trace);trace.append(self.value.split('/')[-1])
    if case['fault'] and case['fault']['index']==index:
     raise PermissionError('injected stat error') if case['fault']['kind']=='permission' else OSError('injected stat error')
    return case['files'][index]
  scope=classes('openpilot/selfdrive/modeld/signal_neural_shadow.py',['run_selected'])
  scope.update(Path=FakePath,run=lambda *args,**kwargs:trace.append('run:qcom'))
  try:scope['run_selected']('/private','qcom');route='qcom'
  except RuntimeError:route='missing'
  except OSError:route='stat_error'
  expected.append(dict(route=route,trace=trace))
 output=dict(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3',scope='original run_selected; only leaf execution and stat boundary inert',cases=[dict(request=request,expected=expected) for request,expected in zip(cases,expected,strict=True)])
 args.output.write_text(json.dumps(output,indent=2)+'\n');print(json.dumps(dict(cases=len(cases),reads=sum(len(value['trace'])-(value['route']=='qcom') for value in expected))))
if __name__=='__main__':main()
