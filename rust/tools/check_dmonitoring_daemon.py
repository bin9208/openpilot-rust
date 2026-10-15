# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
"""Compare the native DM2 daemon over original msgq IPC to the pinned whole dispatcher.

Use the existing native msgq Python binding first on PYTHONPATH. No Python is
started by the native daemon; this is a host validation oracle only.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
from openpilot.cereal import messaging,log
from openpilot.cereal.services import SERVICE_LIST
from dmonitoring_daemon_reference import Oracle,TOPICS,PIN
from check_monitoring_reference import compare


def event(name,now):
    value=messaging.new_message(name);value.valid=True;value.logMonoTime=round(now*1e9);return value

def packets(frame,scenario):
    now=100.+frame*.05
    result={name:event(name,now) for name in TOPICS if name!='carParams'}
    cs=result['carState'].carState
    cs.canValid=True;cs.vEgo=20.;cs.vEgoRaw=20.;cs.gearShifter='drive'
    result['selfdriveState'].selfdriveState.enabled=True
    result['selfdriveState'].selfdriveState.active=True
    result['liveCalibration'].liveCalibration.rpyCalib=[0.,0.,0.]
    md=result['modelV2'].modelV2
    md.meta.disengagePredictions.brakeDisengageProbs=[1.]
    md.orientationRate.z=[0.]*10;md.laneLineProbs=[0.,1.,1.,0.]
    driver=result['driverStateV2'].driverStateV2;driver.frameId=frame
    for side in (driver.leftDriverData,driver.rightDriverData):
        side.faceProb=1.;side.leftEyeProb=1.;side.rightEyeProb=1.
        side.faceOrientation=[0.,0.,0.];side.facePosition=[0.,0.]
        side.faceOrientationStd=[0.,0.,0.];side.facePositionStd=[0.,0.]
        side.phoneProb=1.
    if scenario=='recovery' and 52<=frame<66:
        del result['driverStateV2']
    if scenario=='soft-hold' and 52<=frame<64:
        cs.vEgo=cs.vEgoRaw=0.;cs.standstill=True;cs.softHoldActive=2
    if scenario=='cancel' and 52<=frame<58:
        cs.buttonEvents=[{'type':'cancel','pressed':frame%2==0}]
    return result

def check(args,scenario):
    prefix=f'rust-probe-dm2-{os.getpid()}-{scenario}'
    shm=Path('/dev/shm')/f'msgq_{prefix}';shm.mkdir()
    directory=args.output/scenario;directory.mkdir();params=directory/'params'/prefix;params.mkdir(parents=True)
    values={'DriverMonitoringEnabled':True,'DriverMonitoringMode':int(scenario=='recovery'),
            'DriverMonitoringSessionDisabled':scenario=='disabled','IsRhdDetected':False,
            'AlwaysOnDM':False,'IsDriverViewEnabled':False,'DriverTooDistracted':False}
    for key,value in values.items():(params/key).write_text(str(int(value)))
    os.environ.update(OPENPILOT_PREFIX=prefix,PARAMS_ROOT=str(params.parent),SIMULATION='1',REPLAY='1')
    messaging.reset_context()
    publishers={name:messaging.pub_sock(name) for name in TOPICS if name!='carParams'}
    subscriber=None
    oracle=Oracle(values);trace=[];process=None
    try:
        frames=115 if scenario=='recovery' else 70
        process=subprocess.Popen([str(args.binary),'--frames',str(frames+1)],stderr=subprocess.PIPE)
        assert process.stderr.readline().strip()==b'dmonitoringd: ready'
        subscriber=messaging.sub_sock('driverMonitoringState',timeout=5000,conflate=False)
        # Subscribe after the native publisher owns the queue, before its first poll.
        raw=subscriber.receive();assert raw is not None,'initial DM2 heartbeat timeout'
        actual=log.Event.from_bytes(raw).__enter__()
        expected=oracle.step([],time.monotonic())
        compare(expected.driverMonitoringState.to_dict(verbose=True),actual.driverMonitoringState.to_dict(verbose=True),0.,'initial')
        assert expected.valid==actual.valid
        for frame in range(frames):
            inputs=packets(frame,scenario)
            encoded={name:value.to_bytes() for name,value in inputs.items()}
            for name,payload in encoded.items():publishers[name].send(payload)
            raw=subscriber.receive();assert raw is not None,f'frame {frame} timeout'
            actual=log.Event.from_bytes(raw).__enter__()
            # Route timestamps identify the snapshot actually assessed by the child.
            expected_time=round((100.+frame*.05)*1e9)
            assert actual.logMonoTime==expected_time,(scenario,frame,'input/publication ordering',actual.logMonoTime,expected_time)
            expected=oracle.step(list(encoded.values()),time.monotonic())
            compare(expected.driverMonitoringState.to_dict(verbose=True),actual.driverMonitoringState.to_dict(verbose=True),0.,f'{scenario}[{frame}]')
            assert expected.valid==actual.valid,(scenario,frame,'valid')
            trace.append({'frame':frame,'valid':actual.valid,'state':actual.driverMonitoringState.to_dict(verbose=True)})
        assert process.wait(timeout=5)==0,process.stderr.read().decode()
        if scenario=='recovery':
            assert any(t['state']['cameraUnavailable'] for t in trace[62:70])
            assert not trace[-1]['state']['cameraUnavailable']
        if scenario=='soft-hold':assert all(t['state']['dm2Disabled'] for t in trace[52:64])
        if scenario=='cancel':assert (params/'DriverMonitoringSessionDisabled').read_text()=='1'
        if scenario=='disabled':assert all(t['state']['dm2Disabled'] for t in trace)
        (directory/'trace.json').write_text(json.dumps(trace,indent=2)+'\n')
        report={'scenario':scenario,'compared_publications':frames+1,'packet_tolerance':0.,'source':PIN,'source_hashes':oracle.hashes,'writes':oracle.params.writes,'device_validation':False}
        (directory/'report.json').write_text(json.dumps(report,indent=2)+'\n');return report
    finally:
        if process is not None and process.poll() is None:process.terminate();process.wait(timeout=5)
        if process is not None:(directory/'daemon.log').write_bytes(process.stderr.read())
        publishers.clear();subscriber=None;messaging.reset_context();shutil.rmtree(shm)

def signals(args):
    results=[]
    for sig in (signal.SIGINT,signal.SIGTERM):
        prefix=f'rust-probe-dm2-signal-{os.getpid()}-{sig.name}'
        shm=Path('/dev/shm')/f'msgq_{prefix}';shm.mkdir()
        env=dict(os.environ,OPENPILOT_PREFIX=prefix,PARAMS_ROOT=str(args.output/'signal-params'))
        process=subprocess.Popen([str(args.binary)],env=env,stderr=subprocess.PIPE)
        try:
            assert process.stderr.readline().strip()==b'dmonitoringd: ready'
            process.send_signal(sig);assert process.wait(timeout=5)==0
            results.append(f'{sig.name} camera-absent dispatcher')
        finally:
            if process.poll() is None:process.kill();process.wait(timeout=5)
            process.stderr.close();shutil.rmtree(shm)
    return results

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--scenario',choices=['recovery','soft-hold','cancel','disabled'],action='append');args=parser.parse_args();args.binary=args.binary.resolve();args.output=args.output.resolve();args.output.mkdir(parents=True,exist_ok=False)
    reports=[check(args,name) for name in (args.scenario or ['recovery','soft-hold','cancel','disabled'])]
    report={'runs':reports,'signals':signals(args),'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'device_validation':False}
    (args.output/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report),flush=True)
if __name__=='__main__':main()
