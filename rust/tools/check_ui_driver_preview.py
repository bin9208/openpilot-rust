"""Real VisionIPC and source/native passive inset pixel/lifecycle comparison."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
from PIL import Image, ImageChops
from openpilot.cereal import log
from ui_application_qa.camera_lane import capture
from ui_application_qa.runtime_peer import SERVICES


def packets(index, mode):
  values={'selfdriveState':{'enabled':True,'alertSize':'small' if mode=='alert' else 'none'},
          'driverMonitoringState':{'cameraUnavailable':mode=='unavailable','lockout':mode=='lockout','activePolicy':'wheeltouch' if mode=='wheel' else 'vision',
                                   'isRHD':mode=='rhd','visionPolicyState':{'faceDetected':mode!='search','isDistracted':mode=='distracted'}},
          'driverStateV2':{side:{'facePosition':[.2 if side=='rightDriverData' else -.2,.05]} for side in ('leftDriverData','rightDriverData')}}
  result=[]
  for name in SERVICES:
    event=log.Event.new_message();event.valid=not(mode=='invalid' and name=='driverMonitoringState');event.logMonoTime=int(index/20*1e9) if mode!='stale' else 1
    event.init(name,0) if name in ('pandaStates','onroadEvents','customReservedRawData0') else event.init(name)
    if name in values:event.from_dict({name:values[name]})
    result.append(list(event.to_bytes()))
  return result


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--peer',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--display',default=':139');parser.add_argument('--lane',choices=['both','source','native','compare'],default='both');args=parser.parse_args()
  root=Path(__file__).resolve().parents[2];assert shutil.disk_usage(root).free>1024**3
  args.output.mkdir(parents=True,exist_ok=True);results=[]
  modes=['tracking','rhd','search','distracted','wheel','unavailable','lockout','stale','invalid','alert','offroad','tracking']
  for big in (False,True):
    label='large' if big else 'compact';steps=[]
    for index in range(72):
      mode=modes[index//6];steps.append({'frame':index,'messages':packets(index,mode),'started':mode!='offroad','started_frame':-1,'params':{'IsDriverViewEnabled':'0'},'memory':{}})
    scene={'kind':'driver-preview','config':{'big':big,'large_viewport':big,'pc':True,'scale':1.0},'language':'en','rect':{'x':0,'y':0,'width':2160 if big else 536,'height':1080 if big else 240},'frames':72,'prime':0,'params':{'IsDriverViewEnabled':'0'},'capture_frames':list(range(72)),'background':[0,0,0,255],'camera':{'stream':1},'road':{'steps':steps}}
    path=args.output/f'{label}.json';path.write_text(json.dumps(scene));traces=[]
    for lane in ([] if args.lane=='compare' else ['source','native'] if args.lane=='both' else [args.lane]):
      traces.append(capture(root,args.binary.resolve(),args.peer.resolve(),path,args.output/f'{lane}-{label}.png',args.display,72,lane))
    if args.lane=='compare':
      traces=[json.loads((args.output/f'{lane}-{label}.json').read_text()) for lane in ('source','native')]
    if args.lane in ('both','compare'):
      assert traces[0]==traces[1],(label,'state differs')
      different=[]
      for index in range(72):
        a=Image.open(args.output/f'source-{label}-frame-{index:04}.png').convert('RGB')
        b=Image.open(args.output/f'native-{label}-frame-{index:04}.png').convert('RGB')
        if ImageChops.difference(a,b).getbbox():different.append(index)
      assert not different,(label,different)
    results.append({'layout':label,'frames':72,'driverView_unchanged':True})
  (args.output/'result.json').write_text(json.dumps({'results':results,'lane':args.lane,'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest()},indent=2)+'\n');print(results)
if __name__=='__main__':main()
