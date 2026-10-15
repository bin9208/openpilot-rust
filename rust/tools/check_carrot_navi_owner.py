"""Exercise two simultaneous navigation control connections against an owned daemon."""
import argparse
import asyncio
import json
import os
from pathlib import Path
import sys
import aiohttp
from carrot_navi_cases import query
from carrot_navi_qa.process import Process, available_port

async def main():
  p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--binding',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
  root=Path(__file__).resolve().parents[2];port=available_port();url=f'http://127.0.0.1:{port}';common=['--host','127.0.0.1','--port',str(port),'--no-beacon','--no-cereal'];params=a.output/'params'
  command=[str(a.binary.resolve()),*common] if a.binary else [sys.executable,'-P',str(root/'rust/tools/carrot_navi_process_source.py'),'--binding',str(a.binding.resolve()),'--params-root',str(params.resolve()),*common]
  proc=Process(command,root,a.output,dict(os.environ,PARAMS_ROOT=str(params.resolve())))
  proc.start();rows=[]
  try:
    async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=5)) as http:
      await proc.ready(http,url)
      async with http.ws_connect(url+'/api/navi/ws/v2/control/first') as first:
        await first.send_json({'type':'hello'})
        denied=await first.receive_json();assert denied['code']=='invalid_control_message' and denied['message']=='negotiate navigation control before sending events',denied
        await first.send_json(query());manifest=await first.receive_json();session=manifest['session_id']
        for attempt in range(2):
          async with http.ws_connect(url+'/api/navi/ws/v2/control/second') as second:
            await second.send_json(query());busy=await second.receive_json();assert busy['code']=='receiver_busy',busy
            close=await second.receive();assert close.type==aiohttp.WSMsgType.CLOSE and close.data==1013,close
            rows.append({'busy':busy,'close':close.data})
          async with http.get(url+'/health') as response:
            health=await response.json();assert health['session_id']==session,health
        await first.send_json(query());renewed=await first.receive_json();assert renewed['session_id']!=session
      # Owner cleanup must be complete before replacement negotiation.
      for _ in range(100):
        async with http.get(url+'/health') as response:health=await response.json()
        if health['control_connections']==0:break
        await asyncio.sleep(.01)
      assert health['control_connections']==0
      async with http.ws_connect(url+'/api/navi/ws/v2/control/replacement') as replacement:
        await replacement.send_json(query());manifest=await replacement.receive_json();assert 'session_id' in manifest,manifest
        rows.append({'replacement':True})
    assert await proc.stop()==0
    (a.output/'result.json').write_text(json.dumps({'result':'pass','rows':rows},indent=2)+'\n');print('PASS two-phone ownership, unnegotiated event, re-negotiation and disconnect recovery')
  finally:
    if proc.process.poll() is None:await proc.stop()
if __name__=='__main__':asyncio.run(main())
