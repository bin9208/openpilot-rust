from __future__ import annotations

import base64
import os
from urllib.parse import quote

from .fixture import Fixture

ROUTE='0000001a--aaaaaaaaaa'
UNICODE='차량 !~\"%--１'

def populated(f: Fixture):
  for index in range(7): f.file(f'{ROUTE}--{index}','qcamera.ts',b'owned TS raw marker '+bytes([index]))
  for name,body in (('rlog.zst',b'owned preferred rlog'),('rlog.bz2',b'owned alternative rlog'),('rlog',b'owned plain rlog'),
      ('qlog.zst',b''),('qlog.bz2',b'owned preferred qlog'),('qlog',b'owned alternative qlog'),('qcamera.mp4',b'owned playback MP4')):
    f.file(ROUTE+'--0',name,body)
  for name,body in (('rlog.zst',b''),('rlog.bz2',b'owned fallback rlog'),('qlog.zst',b''),('qlog.bz2',b''),('qlog',b'owned plain qlog'),('qcamera.mp4',b'')):
    f.file(ROUTE+'--1',name,body)
  f.file(ROUTE+'--2','rlog',b'owned uncompressed log')
  target=f.file(ROUTE+'--0','owned-target',b'owned symlink target bytes')
  for index,mtime in ((3,-1),(5,-1500001)):
    path=f.root/f'{ROUTE}--{index}'/'qlog.zst';path.symlink_to(target);os.utime(path,ns=(mtime,mtime),follow_symlinks=False)
  f.file(ROUTE+'--5','rlog',b'owned completed tail')
  f.file(ROUTE+'--6','qlog',b'owned incomplete tail');f.file(ROUTE+'--6','rlog.lock',b'owned lock')
  for name in ('rlog.zst','qlog.zst','qcamera.mp4'):f.file(UNICODE,name,b'owned unicode segment')
  f.file('no-video--0','rlog',b'owned log without video')

async def metadata(f: Fixture):
  summary='/api/dashcam/summary-source/'+ROUTE;replay='/api/dashcam/replay-source/'
  await f.pair('summary-preference-order-skips-nofollow-negative-floor',summary)
  await f.pair('summary-head',summary,'HEAD')
  await f.pair('summary-missing-route-no-cache-header','/api/dashcam/summary-source/missing')
  await f.pair('summary-raw-encoded-slash-route','/api/dashcam/summary-source/'+ROUTE+'%2Fowned')
  for index in (0,1,2,3,4):await f.pair('replay-description-selection-'+str(index),replay+ROUTE+'--'+str(index))
  await f.pair('replay-description-head',replay+ROUTE+'--0','HEAD')
  await f.pair('replay-description-safe-id-normalization',replay+quote(' '+UNICODE+' ',safe=''))
  await f.pair('replay-description-missing-segment',replay+'missing--0')
  await f.pair('replay-description-missing-video',replay+'no-video--0')
  await f.pair('replay-description-invalid-raw-slash',replay+ROUTE+'--0%2Fowned')
  await f.pair('encoded-static-prefix','/api/%64ashcam/%72eplay-source/'+ROUTE+'--0')
  await f.pair('metadata-method-allow',replay+ROUTE+'--0','POST')
  f.file(ROUTE+'--6','rlog',b'owned now-complete log');(f.root/(ROUTE+'--6')/'rlog.lock').unlink()
  await f.pair('summary-newest-tail-child-completion-visible',summary)
  signature=f.root.stat().st_mtime_ns;f.root.chmod(0);os.utime(f.root,ns=(signature+1000000000,signature+1000000000))
  try:await f.pair('summary-generic-filesystem-error-hidden-message',summary)
  finally:f.root.chmod(0o700)
  await f.pair('summary-filesystem-error-recovery',summary)

async def raw(f: Fixture):
  replay='/api/dashcam/replay-source/';download='/api/dashcam/download/'
  for index,kind in ((0,'rlog'),(0,'qlog'),(0,'video'),(1,'rlog'),(1,'qlog'),(1,'video'),(2,'rlog'),(3,'qlog')):
    await f.pair('untouched-replay-file-'+str(index)+'-'+kind,replay+ROUTE+'--'+str(index)+'/'+kind)
  for index,kind in ((0,'qcamera'),(0,'rlog'),(0,'qlog'),(1,'rlog'),(1,'qlog'),(2,'rlog')):
    await f.pair('download-isfile-preference-includes-empty-'+str(index)+'-'+kind,download+ROUTE+'--'+str(index)+'/'+kind)
  await f.pair('download-unicode-raw-attachment-name',download+quote(' '+UNICODE+' ',safe='')+'/rlog')
  await f.pair('download-owned-mp4-mime-override',download+quote(UNICODE,safe='')+'/qcamera')
  await f.pair('raw-kind-python-whitespace',replay+ROUTE+'--0/%1Crlog%1F')
  for base in (replay,download):
    await f.pair('raw-id-encoded-slash-bad-segment',base+ROUTE+'--0%2Fowned/rlog')
    await f.pair('raw-id-missing-directory',base+'missing--0/rlog')
    await f.pair('raw-kind-unknown',base+ROUTE+'--0/unknown')
    await f.pair('raw-file-method-allow',base+ROUTE+'--0/rlog','POST')
  await f.pair('raw-rlog-missing',replay+ROUTE+'--4/rlog')
  await f.pair('raw-qlog-missing',replay+ROUTE+'--2/qlog')
  path=replay+ROUTE+'--0/rlog';response=await f.pair('raw-file-base-etag-last-modified',path)
  await f.pair('raw-file-head-with-presets',path,'HEAD')
  await f.pair('raw-file-range-with-presets',path,headers={'Range':'bytes=1-5'})
  etag=base64.b64decode(response['headers_base64']['etag']).decode()
  await f.pair('raw-file-if-none-match-with-presets',path,headers={'If-None-Match':etag})
  await f.pair('raw-file-if-match-failure-with-presets',path,headers={'If-Match':'"owned-mismatch"'})
  await f.pair('raw-file-unsatisfied-range-with-presets',path,headers={'Range':'bytes=99999-'})
  await f.pair('download-head',download+ROUTE+'--0/qcamera','HEAD')
  candidate=f.root/(ROUTE+'--0')/'rlog.zst';candidate.chmod(0)
  try:await f.pair('raw-file-provider-permission-error-with-presets',path)
  finally:candidate.chmod(0o600)
  await f.pair('raw-file-provider-permission-recovery',path)

async def default_mime(f: Fixture):
  for name,kind in (('qcamera.ts','qcamera'),('qcamera.mp4','qcamera'),('rlog.zst','rlog'),('rlog.bz2','rlog'),('rlog','rlog')):
    segment='default-'+name.replace('.','-')+'--0';f.file(segment,name,b'owned default MIME marker')
    await f.pair('builtin-default-mime-'+name,'/api/dashcam/download/'+segment+'/'+kind)

async def app(f: Fixture):
  await f.pair('application-summary','/api/dashcam/summary-source/'+ROUTE)
  await f.pair('application-replay-description-head','/api/%64ashcam/replay-source/'+ROUTE+'--0','HEAD')
  path='/api/dashcam/replay-source/'+ROUTE+'--0/rlog';response=await f.pair('application-raw-file-range',path,headers={'Range':'bytes=1-5'})
  await f.pair('application-raw-file-conditional',path,headers={'If-None-Match':base64.b64decode(response['headers_base64']['etag']).decode()})
  await f.pair('application-download-unicode','/api/dashcam/download/'+quote(UNICODE,safe='')+'/rlog')
  f.file('negative--0','rlog',b'owned negative timestamp',-1750000000)
  path='/api/dashcam/replay-source/negative--0/rlog';response=await f.pair('application-negative-file-signed-etag-date',path)
  await f.pair('application-negative-file-conditional',path,headers={'If-None-Match':base64.b64decode(response['headers_base64']['etag']).decode()})
  await f.pair('application-negative-file-range',path,headers={'Range':'bytes=0-2'})
  f.file('positive--0','rlog',b'owned positive float boundary',1700000000000000001)
  path='/api/dashcam/replay-source/positive--0/rlog';response=await f.pair('application-positive-float-ceil',path)
  modified=base64.b64decode(response['headers_base64']['last-modified']).decode()
  await f.pair('application-positive-float-if-modified-since',path,headers={'If-Modified-Since':modified})
  await f.pair('application-positive-float-if-range',path,headers={'If-Range':modified,'Range':'bytes=0-2'})
