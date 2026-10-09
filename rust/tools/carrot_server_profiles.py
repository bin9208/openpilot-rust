# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Existing oracle Python/PYTHONPATH only; --binding is the unchanged Params binary.
# python rust/tools/carrot_server_profiles.py --binary rust/target/debug/examples/settings_profiles --binding <params_pyx.so> --output .omo/evidence/225-profiles
import argparse
from copy import deepcopy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
from types import SimpleNamespace
from typing import Iterator, Literal, TypeAlias, TypedDict, assert_never
import uuid

from original_params_binding import load

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']
Action: TypeAlias = Literal['init','read','write','get','snapshot','create','update','delete','preview','apply','restore_preview','restore_apply','restore_raw','git_meta','commit_url','capture','append','history','verify','observe','note','fingerprint','read_baseline','write_baseline','count_since','catalog','put','unavailable_preview']
class Case(TypedDict, total=False):
  action: Action
  data: Json
  values: Json
  id: str
  name: Json
  source: Json
  prev: Json
  next: Json
  engaged: bool
  limit: int
  allowed: Json
  selected: Json
  timestamp: int
  fingerprint: Json
  remote: Json
  commit: Json
  value: Json
  fixture: str
  program: str
  creation_id: str
  now: str

CATALOG = {'params': [
  {'name':'IsMetric','min':0,'max':1,'default':1},
  {'name':'LongitudinalPersonalityMax','min':3,'max':4,'default':3},
  {'name':'CruiseGapLevels','min':2,'max':4,'default':4,'options':{'en':['two','three','four']}},
  {'name':'UptimeOnroad','min':0.0,'max':100.0,'default':0.0},
  {'name':'CarName','default':''}, {'name':'InstallDate','default':''},
  {'name':'LiveParameters','default':{}}, {'name':'CarParamsPersistent','default':''},
  {'name':'FutureSetting','min':0,'max':100,'default':0},
  {'name':'FutureFloat','min':0.0,'max':100.0,'default':0.0},
  {'name':'FutureBool','min':0,'max':1,'default':0},
]}
NOW = '2026-10-08T00:00:00+00:00'
PROFILE_ID = uuid.UUID(int=1).hex


def cases() -> Iterator[Case]:
  yield from [
    {'action':'read'}, {'action':'read','fixture':'corrupt_profiles'},
    {'action':'write','data':{'profiles':[None,{}, {'id':' a ','name':'  한글\n  이름  ','values':{'FutureSetting':14.999,'Ghost':1},'meta':{'branch':3}}, {'id':'a','name':'duplicate','values':{'IsMetric':False}}]}},
    {'action':'get','id':' a '}, {'action':'delete','id':' a '}, {'action':'delete','id':'a'},
    {'action':'snapshot'}, {'action':'create','name':'   '},
    {'action':'create','name':'  🚗\n한국\t profile  '+'x'*60}, {'action':'read'},
    {'action':'update','id':PROFILE_ID,'data':{'name':'  ' }},
    {'action':'update','id':PROFILE_ID,'data':{'values':{'Ghost':1}}},
    {'action':'update','id':PROFILE_ID,'data':{'values':{'FutureSetting':14.999,'FutureFloat':0.5,'FutureBool':'invalid','IsMetric':False,'CarName':'한글','InstallDate':'2026-10-08T00:00:00','LiveParameters':[],'CarParamsPersistent':'raw'}}},
    {'action':'preview','id':PROFILE_ID}, {'action':'apply','id':PROFILE_ID},
    {'action':'preview','id':PROFILE_ID}, {'action':'verify'}, {'action':'history','limit':30},
    {'action':'apply','id':PROFILE_ID,'values':{}},
    {'action':'preview','id':'missing'}, {'action':'update','id':'missing','data':{}}, {'action':'delete','id':'missing'},
    {'action':'update','id':PROFILE_ID,'data':{'name':'한\ud800'}}, {'action':'read'},
    {'action':'update','id':PROFILE_ID,'data':{'name':'회복','values':{'CarName':'\ud800'}}},
    {'action':'update','id':PROFILE_ID,'data':{'name':'회복','values':{'CarName':'restored'}}},
    {'action':'apply','id':PROFILE_ID}, {'action':'read'},
    {'action':'restore_preview','values':{'\ud800':1,'Ghost':2,'FutureSetting':'invalid','IsMetric':'invalid','UptimeOnroad':'bad','LiveParameters':{},'CarParamsPersistent':'x'}},
    {'action':'restore_preview','values':{'FutureSetting':15,'FutureFloat':0.5000005,'IsMetric':'off','CarName':'next'},'selected':['CarName']},
    {'action':'restore_apply','values':{'FutureSetting':15,'CarName':'next'},'selected':['CarName'],'source':'restore'},
    {'action':'restore_apply','values':{'FutureSetting':20},'source':'invalid'},
    {'action':'restore_raw','values':{'Ghost':1,'LiveParameters':{},'InstallDate':'2026-10-08T00:00:00','FutureSetting':'bad'},'source':'restore'},
    {'action':'unavailable_preview','values':{'IsMetric':False}},
    {'action':'write','data':{'profiles':[{'id':str(i),'name':'p','values':{'FutureSetting':i}} for i in range(42)]}},
    {'action':'create','name':'at limit'}, {'action':'write','data':{'profiles':None}},
    {'action':'write','data':{'profiles':[]}}, {'action':'create','name':'after recovery'},
    {'action':'observe','values':{'A':1,'B':False}}, {'action':'observe','values':{'A':True,'B':0}},
    {'action':'observe','values':{'A':2,'B':3}}, {'action':'append','name':'A','prev':2,'next':4,'source':'web_ui','engaged':True},
    {'action':'observe','values':{'A':4,'B':3}}, {'action':'observe','values':{'A':5,'B':4},'allowed':{'A':None}},
    {'action':'append','name':' ','prev':1,'next':2}, {'action':'history','limit':3,'name':'A'},
    {'action':'history','limit':2,'source':'device'}, {'action':'verify'},
    {'action':'fingerprint','values':{'B':2,'A':1}}, {'action':'fingerprint','values':{'A':1,'B':2}},
    {'action':'read_baseline'}, {'action':'write_baseline','fingerprint':'한글'}, {'action':'read_baseline'},
    {'action':'count_since','timestamp':1000}, {'action':'count_since','timestamp':1001},
    {'action':'verify','fixture':'corrupt_log'}, {'action':'append','name':'Recovery','prev':0,'next':1,'source':'device'},
    {'action':'append','name':'Ring','prev':1001,'next':1002,'source':'profile','fixture':'ring_log'}, {'action':'verify'}, {'action':'history','limit':2},
    {'action':'git_meta','fixture':'large_remote'}, {'action':'git_meta','fixture':'normal_remote'},
    {'action':'git_meta','fixture':'invalid_utf8_git'}, {'action':'git_meta','fixture':'timeout_git'},
    {'action':'capture'},
  ]
  for remote in ('https://github.com/o/r.git/','http://github.com/o/r','git@github.com:o/r.git','git@github.com:o/nested/r','https://gitlab.com/o/r','https://github.com/o/.git'):
    yield {'action':'commit_url','remote':remote,'commit':' abc '}


def main() -> None:
  parser=argparse.ArgumentParser(); parser.add_argument('--binary',type=Path);parser.add_argument('--binding',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
  args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
  roots=[args.output/name for name in ('original','native')]
  for root in roots: root.mkdir(exist_ok=True);(root/'state').mkdir(exist_ok=True)
  repo=args.output/'owned-git';repo.mkdir(exist_ok=True)
  for command in (['git','init','-b','fixture'],['git','config','user.email','fixture@example.invalid'],['git','config','user.name','Fixture'],['git','commit','--allow-empty','-m','fixture'],['git','remote','add','origin','https://github.com/fixture/owned.git']):
    subprocess.run(command,cwd=repo,check=True,capture_output=True)
  load(args.binding.resolve(),f'ipc://{args.output.resolve()}/logs.sock',args.output/'binding-logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import param_changes as history,params,setting_profiles as profiles,settings
  source_params=Params(str((roots[0]/'params').resolve()));native_params=Params(str((roots[1]/'params').resolve()))
  for store in (source_params,native_params):
    for name,value in {'IsMetric':b'1','LongitudinalPersonalityMax':b'3','CruiseGapLevels':b'4','UptimeOnroad':b'0.1','CarName':b'old','InstallDate':b'2026-10-08T00:00:00','LiveParameters':b'{}','CarParamsPersistent':b'raw'}.items():Path(store.get_param_path(name)).write_bytes(value)
  params.Params=lambda:source_params
  catalog_path=args.output/'catalog.json';catalog_path.write_text(json.dumps(CATALOG));settings.settings_cache.update(path=str(catalog_path.resolve()),data=None)
  profiles.CARROT_SETTING_PROFILES_PATH=str((roots[0]/'state/setting_profiles.json').resolve());profiles.REPO_DIR=str(repo.resolve());profiles._now_iso=lambda:NOW;profiles.uuid=SimpleNamespace(uuid4=lambda:uuid.UUID(hex=PROFILE_ID))
  history.CARROT_PARAM_CHANGES_PATH=str((roots[0]/'state/param_changes.jsonl').resolve());history.CARROT_FINGERPRINT_BASELINE_PATH=str((roots[0]/'state/fingerprint_baseline.json').resolve());history.time=SimpleNamespace(time=lambda:1000);history._known_values.clear()

  def source(case:Case)->Json:
    match case['action']:
      case 'init':return None
      case 'read':return profiles.read_setting_profiles()
      case 'write':return profiles.write_setting_profiles(case['data'])
      case 'get':return profiles.get_setting_profile(case['id'])
      case 'snapshot':return profiles.snapshot_current_setting_values()
      case 'create':return profiles.create_setting_profile(case['name'])
      case 'update':return profiles.update_setting_profile(case['id'],case['data'])
      case 'delete':profiles.delete_setting_profile(case['id']);return None
      case 'preview':return profiles.preview_setting_profile(case['id'],case.get('values'))
      case 'apply':return profiles.apply_setting_profile(case['id'],case.get('values'))
      case 'restore_preview':return params.preview_param_restore_values(case['values'],case.get('selected'))
      case 'restore_apply':return params.restore_param_values_validated(case['values'],case.get('selected'),case.get('source','restore'))
      case 'restore_raw':return params.restore_param_values_from_backup(case['values'],case.get('source','restore'))
      case 'unavailable_preview':
        saved=params.HAS_PARAMS;params.HAS_PARAMS=False
        try:return params.preview_param_restore_values(case['values'])
        finally:params.HAS_PARAMS=saved
      case 'git_meta':return profiles.read_git_profile_meta()
      case 'commit_url':return profiles._commit_url(case['remote'],case['commit'])
      case 'capture':return {'id':uuid.uuid4().hex,'now':datetime.now(timezone.utc).replace(microsecond=0).isoformat(),'meta':profiles.read_git_profile_meta()}
      case 'append':return history.append_param_change(case.get('name',''),case.get('prev'),case.get('next'),case.get('source','web_ui'),case.get('engaged',False))
      case 'history':return history.read_param_changes(case.get('limit',0),case.get('name',''),case.get('source',''))
      case 'verify':return history.verify_param_changes()
      case 'observe':return history.observe_param_values(case['values'],set(case['allowed']) if case.get('allowed') else None)
      case 'note':history.note_known_value(case['name'],case['value']);return None
      case 'fingerprint':return history.param_fingerprint(case['values'])
      case 'read_baseline':return history.read_fingerprint_baseline()
      case 'write_baseline':return history.write_fingerprint_baseline(case['fingerprint'])
      case 'count_since':return history.count_changes_since(case['timestamp'],set(case['allowed']) if case.get('allowed') else None)
      case 'catalog':catalog_path.write_text(json.dumps(case['data']));settings.settings_cache['data']=None;return None
      case 'put':return params.set_param_value(case['name'],case['value'],settings.get_settings_cached()[2].get(case['name']))
      case unknown:assert_never(unknown)

  def prepare(case:Case)->Case:
    fixture=case.get('fixture','');case=dict(case)
    if fixture in ('corrupt_profiles','corrupt_log'):
      filename='setting_profiles.json' if fixture=='corrupt_profiles' else 'param_changes.jsonl'
      for root in roots:(root/'state'/filename).write_text('{broken\n')
    elif fixture=='ring_log':
      records=[];previous=history.GENESIS_HASH
      for index in range(1001):
        record={'ts':1000,'name':'Ring','prev':index,'next':index+1,'source':'profile','engaged':False,'prev_hash':previous};record['hash']=history.record_hash(record);previous=record['hash'];records.append(history._canonical(record)+'\n')
      for root in roots:(root/'state/param_changes.jsonl').write_text(''.join(records))
    elif fixture in ('large_remote','normal_remote'):
      remote='https://github.com/fixture/'+('x'*98304 if fixture=='large_remote' else 'owned')+'.git';subprocess.run(['git','config','remote.origin.url',remote],cwd=repo,check=True)
    elif fixture in ('invalid_utf8_git','timeout_git'):
      directory=args.output/fixture;directory.mkdir(exist_ok=True);program=directory/'git'
      program.write_text('#!/bin/sh\nif [ "$1" = branch ]; then '+("printf '\\377'" if fixture=='invalid_utf8_git' else 'exec sleep 4')+'; fi\n');program.chmod(0o700);case['program']=str(program.resolve())
    case['creation_id']=PROFILE_ID;case['now']=NOW
    return case

  def snapshot(root:Path,store)->Json:
    names=['setting_profiles.json','setting_profiles.json.tmp','param_changes.jsonl','param_changes.jsonl.tmp','fingerprint_baseline.json','fingerprint_baseline.json.tmp']
    return {'files':{name:(root/'state'/name).read_bytes().hex() if (root/'state'/name).is_file() else None for name in names},'params':{item['name']:Path(store.get_param_path(item['name'])).read_bytes().hex() if Path(store.get_param_path(item['name'])).is_file() else None for item in CATALOG['params']}}

  init={'action':'init','root':str((roots[1]/'params').resolve()),'state':str((roots[1]/'state').resolve()),'repository':str(repo.resolve()),'git_program':'/usr/bin/git','catalog':CATALOG,'timestamp':1000}
  inputs=[init,*list(cases())]
  with (args.output/'inputs.jsonl').open('w') as requests,(args.output/'original.jsonl').open('w') as expected,(args.output/'native.jsonl').open('w') as actual,(args.output/'native.stderr').open('w') as stderr:
    process=subprocess.Popen([str(args.binary.resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,text=True) if args.binary else None
    try:
      for index,raw in enumerate(inputs):
        case=prepare(raw);old_path=os.environ['PATH']
        if 'program' in case:os.environ['PATH']=str(Path(case['program']).parent)+os.pathsep+old_path
        try:
          try:wanted=source(case)
          except profiles.SettingProfileError as error:wanted={'error':str(error),'error_code':error.code}
          except (ValueError,TypeError,KeyError,RuntimeError,OSError,AttributeError,OverflowError) as error:wanted={'error':str(error)}
        finally:os.environ['PATH']=old_path
        request=json.dumps(case);requests.write(request+'\n');record={'output':wanted,**snapshot(roots[0],source_params)};expected.write(json.dumps(record)+'\n');expected.flush()
        if process:
          assert process.stdin is not None and process.stdout is not None
          process.stdin.write(request+'\n');process.stdin.flush();result={'output':json.loads(process.stdout.readline()),**snapshot(roots[1],native_params)};actual.write(json.dumps(result)+'\n');actual.flush()
          if case['action']=='capture':
            for output in (wanted,result['output']):assert uuid.UUID(hex=output['id']).version==4 and output['now'].endswith('+00:00')
            assert abs((datetime.fromisoformat(wanted['now'])-datetime.fromisoformat(result['output']['now'])).total_seconds())<3
            record['output']={**wanted,'id':'UUID4','now':'SYSTEM_UTC'};result['output']={**result['output'],'id':'UUID4','now':'SYSTEM_UTC'}
          assert result==record,f'case {index} {case["action"]} mismatch; retained source/native output and filesystem/Params bytes'
    finally:
      if process:
        assert process.stdin is not None
        process.stdin.close();assert process.wait(timeout=10)==0
  receipt={'cases':len(inputs),'passed':bool(args.binary),'comparison':'original returns/coded errors + saved/temp history/profile files + actual Params bytes; system UUID/time properties validated','source_only':not bool(args.binary)}
  (args.output/'result.json').write_text(json.dumps(receipt)+'\n');print(json.dumps(receipt))


if __name__=='__main__':main()
