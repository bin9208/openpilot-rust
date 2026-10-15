"""Pinned source atomic publication payload oracle, separate control endpoints."""
import argparse,ast,json,math,subprocess
from pathlib import Path
from signal_source import source,compare

def publisher(path,name):
  nodes=[n for n in ast.parse(source(path)).body if isinstance(n,ast.FunctionDef) and n.name==name]
  assert len(nodes)==1
  scope={'Path':Path,'math':math,'json':json,'MAX_BYTES':8192 if name=='publish_display' else 32768,'SIZE':(1344,760),'ENABLE_PATH':Path('/unused/enabled'),'OBSERVATION_PATH':Path('/unused/obs'),'DISPLAY_PATH':Path('/unused/display'),'requested':lambda path:True}
  exec(compile(ast.Module(body=nodes,type_ignores=[]),'e8e152db:'+path,'exec'),scope)
  return scope[name]

def main():
  p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--output',type=Path,required=True);args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  start=publisher('openpilot/selfdrive/carrot/signal_start_guard.py','publish_start_observation');prepare=publisher('openpilot/selfdrive/carrot/signal_stop_prepare_runtime.py','publish_prepare_observation');display=publisher('openpilot/selfdrive/carrot/signal_display.py','publish_display')
  cases=[];expected=[]
  for i in range(128):
    record={'timestamp_eof':10_000_000_000+i*50_000_000,'frame_id':i,'session':'source:'+str(i//32),'neural':i%2==1,'backend':['cpu','qcom'][i%2],'revalidated':i%3==0,'daytime_comparison':i%4==0,'tracks':[]}
    for j in range(i%24):
      track={'id':j+1,'box':[600+j,200,640+j,210],'state':['red','green','unknown'][j%3],'age':[0.,.1,.5][j%3],'observations':j%5,'seen_red':j%2==0,'evidence':{'raw':['red','green','unknown'][j%3]},'score':[.79,.8,.9,1.,1.01][j%5],'neural_age':[0.,.5,.51][j%3],'label':['red','green','yellow','green,red','green,yellow'][j%5]}
      if j%7==0:track.pop('label')
      if j%11==0:track.pop('neural_age')
      record['tracks'].append(track)
    out={}
    for name,fn in [('start',start),('prepare',prepare),('display',display)]:
      path=args.output/f'{name}.json';fn(record,path=path);out[name]=json.loads(path.read_text())
    cases.append(record);expected.append(out)
  (args.output/'input.json').write_text(json.dumps(cases));(args.output/'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw);(args.output/'receipt.json').write_text(json.dumps(compare(json.loads(raw),expected))+'\n')
  print(json.dumps({'records':len(cases),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
