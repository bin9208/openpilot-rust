"""Actual production automatic backend source; DM validity never grants GPU."""
import argparse,ast,itertools,json,subprocess,sys
from pathlib import Path
from types import ModuleType
from signal_source import source,compare

def main():
  p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
  mode=ModuleType('openpilot.selfdrive.carrot.signal_mode');mode.read_mode=lambda params:params.values['mode'];sys.modules[mode.__name__]=mode
  node=next(n for n in ast.parse(source('openpilot/selfdrive/modeld/signal_neural_shadow.py')).body if isinstance(n,ast.FunctionDef) and n.name=='automatic_backend');scope={};exec(compile(ast.Module(body=[node],type_ignores=[]),'e8e152db:automatic_backend','exec'),scope)
  class Params:
    def __init__(self,values):self.values=values
    def get_bool(self,name):return self.values[name]
  cases=[];expected=[]
  for mode,*flags in itertools.product(range(4),*[range(2)]*4):
    case=dict(zip(['mode','usb_gpu_active','dm_enabled','dm_session_disabled','driver_view'],[mode,*map(bool,flags)]));cases.append(case)
    values=dict(zip(['UsbGpuActive','DriverMonitoringEnabled','DriverMonitoringSessionDisabled','IsDriverViewEnabled'],map(bool,flags)));values['mode']=mode
    expected.append(scope['automatic_backend'](Params(values)))
  (a.output/'input.json').write_text(json.dumps(cases));(a.output/'expected.json').write_text(json.dumps(expected))
  if a.binary:
    raw=subprocess.check_output([str(a.binary.resolve())],input=json.dumps(cases).encode());(a.output/'actual.json').write_bytes(raw);(a.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'cases':len(cases),'native_compared':bool(a.binary)}))
if __name__=='__main__':main()
