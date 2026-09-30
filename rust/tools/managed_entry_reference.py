#!/usr/bin/env python3
"""Unchanged launcher/Sentry policy with controlled entrypoint and real Params/logging/msgq."""
from __future__ import annotations
import ast
import json
import logging
import os
from pathlib import Path
import sys
import time
import traceback
from types import SimpleNamespace

import msgq
import openpilot.cereal.messaging as messaging
from openpilot.common.logging_extra import SwagLogger
from logging_producer_reference import original_socket_handler, console_handler
import sentry_sdk as real_sdk
import sentry_sdk.integrations.threading as real_threading
from sentry_sdk.integrations.threading import ThreadingIntegration
from setproctitle import setproctitle
from tombstoned_reference import Worker

ROOT=Path(__file__).resolve().parents[2]


def line(value): print(json.dumps(value),flush=True)
def acknowledge(): assert sys.stdin.readline().strip()=='continue'


def run(request,binding):
  root=Path(request['root']);prefix=request['prefix'];steps=[];calls=[];stage='Prepare';initial=None;publisher=None;prepared=None
  worker=Worker(binding)
  sys.modules["sentry_sdk"]=real_sdk
  sys.modules["sentry_sdk.integrations.threading"]=real_threading
  real_sdk.integrations.threading=real_threading
  os.environ['OPENPILOT_PREFIX']=prefix
  worker.handle({'op':'configure','base':str(root/'base'),'params':str(root/'params'),'pc':False,'device':'tici'})
  logger=SwagLogger();logger.setLevel(logging.DEBUG)
  console,_=console_handler('warning');console.setStream(sys.stderr);logger.addHandler(console)
  handler=original_socket_handler(request['endpoint'],logger);logger.addHandler(handler)
  logger.bind_global(inherited='global')
  if request.get('global_daemon') is not None:logger.bind_global(daemon=request['global_daemon'])
  logger.bind(local='retained')
  worker.sentry.cloudlog=logger;worker.version.cloudlog=logger
  failure=None
  def call(operation,fields):
    path=root/'params'/prefix/'CarrotException'
    calls.append({'op':operation,'fields':fields,'carrot_exception':path.read_text() if path.is_file() else None})
    if failure==operation:raise RuntimeError(operation+' fixture failure')
  def init(dsn,**kwargs):
    call('init',{})
    if request.get('real_sdk'):real_sdk.init(request['dsn'],**kwargs)
  def set_user(value):
    call('set_user',value)
    if request.get('real_sdk'):real_sdk.set_user(value)
  def set_tag(key,value):
    nonlocal stage,initial
    if key=='daemon':stage='Tag'
    try:call('daemon_tag' if key=='daemon' else 'set_tag',{'key':key,'value_json':json.dumps(value)})
    except Exception as error:initial=error;raise
    if request.get('real_sdk'):real_sdk.set_tag(key,value)
  def capture(*args,**kwargs):
    error=sys.exc_info()[1]
    call('capture_exception',{'kind':type(error).__name__,'message':str(error),'traceback':traceback.format_exc()})
    if request.get('real_sdk'):real_sdk.capture_exception(*args,**kwargs)
  def flush():
    call('flush',None)
    if request.get('real_sdk'):real_sdk.flush()
  worker.sentry.sentry_sdk=SimpleNamespace(init=init,set_user=set_user,set_tag=set_tag,capture_exception=capture,flush=flush)
  worker.sentry.ThreadingIntegration=ThreadingIntegration
  enabled=worker.sentry.init(worker.sentry.SentryProject.SELFDRIVE)
  calls.clear();failure=request.get('sdk_failure')
  logger.debug('managed-entry-ready');line({'ready':True,'reporting_enabled':enabled});acknowledge()
  def missing():return (root/'missing-input').read_bytes()
  def gate(mode):
    if mode=='interrupt':raise KeyboardInterrupt
    if mode=='error':missing()
    assert mode is None,mode
  def imported(process):
    nonlocal prepared,stage,initial
    stage='Prepare';steps.append({'step':'prepare'})
    try:gate(request.get('prepare'));prepared=(root/'prepared-input').read_bytes()
    except Exception as error:initial=error;raise
    return SimpleNamespace(main=body)
  def title(process):
    nonlocal stage,initial
    stage='Name'
    try:setproctitle(process)
    except Exception as error:initial=error;raise
  def reset():
    nonlocal stage,initial,publisher
    stage='ResetContext';steps.append({'step':'reset_context','comm':Path('/proc/self/comm').read_text().strip()})
    try:
      gate(request.get('reset'))
      previous=msgq.context;messaging.reset_context();assert msgq.context is not previous
      publisher=msgq.pub_sock('managedEntryBody',segment_size=1024*1024)
      line({'context_ready':True,'context_replaced':msgq.context is not previous});acknowledge()
    except Exception as error:initial=error;raise
  def body():
    nonlocal stage,initial
    stage='Body';steps.append({'step':'body','prepared':prepared.decode()})
    try:
      logger.info('managed body entered');publisher.send(b'managed body IPC')
      if request.get('close_logger'):handler.sock.close()
      if request.get('params_fault')=='open':worker.params_root=root/'params-blocked'
      elif request.get('params_fault')=='put':(root/'params').chmod(0o500)
      match request['body']:
        case 'return':return
        case 'error':missing()
        case 'chain':
          try:missing()
          except OSError as error:raise RuntimeError('failed to load managed body fixture') from error
        case 'interrupt':raise KeyboardInterrupt
        case 'signal':
          line({'waiting_for_sigint':True})
          while True:time.sleep(.005)
        case _:raise AssertionError(request['body'])
    except Exception as error:initial=error;raise
  path=ROOT/'openpilot/system/manager/process.py';tree=ast.parse(path.read_text());definition=next(node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name=='launcher')
  namespace={'importlib':SimpleNamespace(import_module=imported),'setproctitle':title,'messaging':SimpleNamespace(reset_context=reset),'cloudlog':logger,'sentry':worker.sentry}
  exec(compile(ast.Module(body=[definition],type_ignores=[]),str(path),'exec'),namespace)
  try:
    namespace['launcher'](request['process'],request['daemon'])
    outcome={'kind':'interrupted' if request['body'] in ['interrupt','signal'] or request.get('prepare')=='interrupt' or request.get('reset')=='interrupt' else 'returned'};success=True
  except Exception as error:
    kind='interrupt_log_failed' if initial is None else ('raised' if error is initial else 'reporting_failed')
    outcome={'kind':kind,'stage':stage,'exception_type':type(error).__name__,'message':str(error),'traceback':traceback.format_exc()};success=False
  line({'outcome':outcome,'steps':steps,'sdk_calls':calls,'comm':Path('/proc/self/comm').read_text().strip(),'cmdline':Path('/proc/self/cmdline').read_bytes().decode(errors='replace').replace('\0',' ')})
  acknowledge();handler.close()
  return 0 if success else 1

if __name__=='__main__':sys.exit(run(json.loads(Path(sys.argv[1]).read_text()),Path(sys.argv[2])))
