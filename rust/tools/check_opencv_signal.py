"""Pinned OpenCV primitive oracle for the native camera signal observer."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import cv2
import numpy as np
from signal_source import compare


def sanitize(value):
  if isinstance(value,float) and not math.isfinite(value): return None
  if isinstance(value,dict):return {k:sanitize(v) for k,v in value.items()}
  if isinstance(value,list):return [sanitize(v) for v in value]
  return value


def main():
  parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path);parser.add_argument('--output',type=Path,required=True)
  parser.add_argument('--disable-ipp-diagnostic',action='store_true',help='Explicit external-backend diagnostic; not the production source gate')
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
  assert cv2.__version__=='4.13.0';cv2.setNumThreads(1);cv2.ocl.setUseOpenCL(False)
  if args.disable_ipp_diagnostic:cv2.ipp.setUseIPP(False)
  rng=np.random.default_rng(7122);cases=[];expected=[]
  for index in range(12):
    h,w=32,48
    rgb=rng.integers(0,256,(h,w,3),np.uint8);gray=cv2.cvtColor(rgb,cv2.COLOR_RGB2GRAY)
    mask=(rng.random((h,w))>.97).astype(np.uint8);mask[5:14,6:18]=1;mask[18:25,26:40]=1
    if index==1:mask[:]=0
    if index==2:mask[:]=1
    patch=gray[3:9,4:12].copy();filter_=np.ones(patch.shape,np.uint8);filter_[2:4,3:5]=0
    if index%2:filter_=None
    count,labels,stats,centers=cv2.connectedComponentsWithStats(mask,8)
    opened=cv2.morphologyEx(mask,cv2.MORPH_OPEN,np.ones((2,3),np.uint8))
    contours,_=cv2.findContours(opened,cv2.RETR_EXTERNAL,cv2.CHAIN_APPROX_SIMPLE)
    scores=cv2.matchTemplate(gray,patch,cv2.TM_CCOEFF_NORMED,mask=filter_)
    expected.append(sanitize({'gray':gray.ravel().tolist(),'hsv':cv2.cvtColor(rgb,cv2.COLOR_RGB2HSV).ravel().tolist(),
      'labels':labels.ravel().tolist(),'stats':[{'bounds':stats[k,:4].tolist(),'area':int(stats[k,4]),'center':centers[k].tolist()} for k in range(count)],
      'opened':opened.ravel().tolist(),'contours':[{'bounds':list(cv2.boundingRect(c)),'area':cv2.contourArea(c)} for c in contours],
      'scores':scores.ravel().tolist(),'tophat':cv2.morphologyEx(mask,cv2.MORPH_TOPHAT,np.ones((15,15),np.uint8)).ravel().tolist(),'distance':cv2.distanceTransform(mask,cv2.DIST_L2,5).ravel().tolist()}))
    cases.append({'width':w,'height':h,'rgb':rgb.ravel().tolist(),'mask':mask.ravel().tolist(),'template':patch.ravel().tolist(),
                  'template_width':8,'template_height':6,'template_mask':[] if filter_ is None else filter_.ravel().tolist()})
  (args.output/'input.json').write_text(json.dumps(cases)+'\n');(args.output/'expected.json').write_text(json.dumps(expected)+'\n')
  if args.binary:
    raw=subprocess.check_output([str(args.binary.resolve())],input=json.dumps(cases).encode());(args.output/'actual.json').write_bytes(raw)
    actual=json.loads(raw);receipt={}
    for key in expected[0]:receipt[key]=compare([v[key] for v in actual],[v[key] for v in expected],tolerance=1e-6 if key=='scores' else 0.)
    (args.output/'receipt.json').write_text(json.dumps(receipt)+'\n')
  print(json.dumps({'opencv':cv2.__version__,'ipp_enabled':cv2.ipp.useIPP(),'diagnostic_only':args.disable_ipp_diagnostic,'cases':len(cases),'native_compared':bool(args.binary)}))
if __name__=='__main__':main()
