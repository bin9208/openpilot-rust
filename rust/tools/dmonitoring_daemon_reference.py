"""Pinned final-source DM2 dispatcher oracle; native runtime never imports this tool."""
from __future__ import annotations
import ast
import hashlib
import math
import os
import subprocess
import time
from collections import deque
from dataclasses import dataclass
from types import SimpleNamespace
from pathlib import Path
from check_message_state import source as message_source
from check_monitoring_reference import original_class, Parameters, message, ROOT, SOURCES
from openpilot.cereal import car, log
from openpilot.cereal.services import SERVICE_LIST

PIN = 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
TOPICS = ('carState','selfdriveState','modelV2','radarState','liveCalibration','carParams','driverStateV2')

def pinned(path):
    return subprocess.check_output(['git','show',f'{PIN}:{path}'],cwd=ROOT,text=True)

def closure():
    paths = [*SOURCES,'openpilot/cereal/messaging/__init__.py',
             *[f'openpilot/selfdrive/monitoring/{name}.py' for name in ('config','dm2','dm2_context','dm2d','dm2_cadence')]]
    result = {}
    for path in paths:
        text = pinned(path)
        result[path] = hashlib.sha256(text.encode()).hexdigest()
        # Native schema retains pre-existing custom events; map policy enums by name.
        if (path in SOURCES and not path.endswith('.capnp')):
            assert (ROOT/path).read_text() == text, f'original-class dependency differs from pin: {path}'
    return result

class LoopParams:
    def __init__(self,values): self.values=dict(values);self.writes=[]
    def get_bool(self,name): return bool(self.values.get(name,False))
    def get_int(self,name): return int(self.values.get(name,0))
    def put_bool(self,name,value): self.values[name]=value;self.writes.append((name,value))
    put_bool_nonblocking=put_bool

class Ratekeeper:
    def __init__(self,*args,**kwargs): self.frame=0
    def keep_time(self): self.frame+=1

class Oracle:
    """Execute the original run_dm2 initialization and one whole while body per native publication."""
    def __init__(self,values):
        self.hashes=closure()
        Parameters.initial=values.get('DriverTooDistracted',False)
        self.params=LoopParams(values);self.packet=None;self.host_now=time.monotonic()
        messaging,environment=message_source();environment['simulation']='1'
        tree=ast.parse(pinned('openpilot/cereal/messaging/__init__.py'));tree.body=[n for n in tree.body if isinstance(n,(ast.FunctionDef,ast.ClassDef)) and n.name in ('new_message','FrequencyTracker','SubMaster')]
        exec(compile(tree,'final-source:messaging','exec'),messaging)
        self.sm=messaging['SubMaster'](list(TOPICS),poll='modelV2')
        self.queues={'carState':[],'carControl':[]}
        def drain(name,**kwargs):
            result=self.queues[name];self.queues[name]=[];return result
        adapter=SimpleNamespace(new_message=message,SubMaster=lambda *a,**kw:self.sm,
            PubMaster=lambda *a:SimpleNamespace(send=self.publish),sub_sock=lambda name,**kw:name,drain_sock=drain)
        ns={'math':math,'deque':deque,'dataclass':dataclass,'DT_DMON':.05,
            'DriverMonitoring':original_class(),'AlertLevel':log.DriverMonitoringState.AlertLevel,
            'MonitoringPolicy':log.DriverMonitoringState.MonitoringPolicy,'car':car,'SERVICE_LIST':SERVICE_LIST,
            'messaging':adapter,'time':SimpleNamespace(monotonic=lambda:self.host_now),
            'os':SimpleNamespace(environ={'REPLAY':'1'}),'Ratekeeper':Ratekeeper,'DmRatekeeper':Ratekeeper,
            'CommandReader':lambda *a:SimpleNamespace(read=lambda **kw:None)}
        for name in ('config','dm2','dm2_context','dm2d'):
            path=f'openpilot/selfdrive/monitoring/{name}.py'
            tree=ast.parse(pinned(path));tree.body=[n for n in tree.body if not isinstance(n,(ast.Import,ast.ImportFrom)) and not (isinstance(n,ast.If) and '__name__' in ast.unparse(n.test))]
            exec(compile(tree,f'{PIN}:{path}','exec'),ns)
        function=next(n for n in ast.parse(pinned('openpilot/selfdrive/monitoring/dm2d.py')).body if isinstance(n,ast.FunctionDef) and n.name=='run_dm2')
        loop=next(n for n in function.body if isinstance(n,ast.While))
        self.scope=dict(ns,params=self.params,experimental=self.params.get_int('DriverMonitoringMode')==1,initial_car_params=None)
        initial=function.body[:function.body.index(loop)]
        exec(compile(ast.Module(body=initial,type_ignores=[]),'final-dm2d:init','exec'),self.scope)
        # Replace only external blocking receive; every original dispatcher statement remains.
        body=loop.body[1:]
        once=ast.For(target=ast.Name(id='_once',ctx=ast.Store()),iter=ast.Tuple(elts=[ast.Constant(None)],ctx=ast.Load()),body=body,orelse=[])
        self.code=compile(ast.fix_missing_locations(ast.Module(body=[once],type_ignores=[])),'final-dm2d:tick','exec')
    def publish(self,topic,packet):
        assert topic=='driverMonitoringState' and self.packet is None
        self.packet=packet
    def step(self,packets,host_now):
        self.host_now=host_now;self.packet=None
        decoded=[log.Event.from_bytes(packet).__enter__() for packet in packets]
        self.sm.update_msgs(host_now,decoded)
        self.queues['carState'] += [p for p in decoded if p.which()=='carState']
        self.queues['carControl'] += [p for p in decoded if p.which()=='carControl']
        exec(self.code,self.scope)
        return self.packet
