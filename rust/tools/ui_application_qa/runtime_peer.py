from __future__ import annotations

import json
from pathlib import Path
import subprocess
import threading
import time
from typing import Final
from openpilot.cereal import log, messaging

SERVICES: Final = (
  'modelV2',
  'controlsState',
  'onroadEvents',
  'liveCalibration',
  'radarState',
  'deviceState',
  'pandaStates',
  'carParams',
  'driverMonitoringState',
  'carState',
  'driverStateV2',
  'roadCameraState',
  'wideRoadCameraState',
  'managerState',
  'selfdriveState',
  'longitudinalPlan',
  'gpsLocationExternal',
  'carOutput',
  'carControl',
  'liveParameters',
  'rawAudioData',
  'carrotMan',
  'carrotNavi',
  'peripheralState',
  'liveDelay',
  'liveTorqueParameters',
  'lateralPlan',
  'customReservedRawData0',
)


class Publisher:
  def __init__(self, vision_peer: Path, environment: dict[str, str]) -> None:
    self.vision_peer = vision_peer
    self.environment = environment
    self.started = threading.Event()
    self.stop = threading.Event()
    self.lock = threading.Lock()
    self.onroad = False
    self.standstill = True
    self.command_index = 0
    self.command = ''
    self.argument = ''
    self.debug_records: list[str] = []
    self.bookmark_received = threading.Event()
    self.completed = False
    self.thread = threading.Thread(target=self.run, name='runtime-fixture-publisher')

  def run(self) -> None:
    with subprocess.Popen(
      [str(self.vision_peer)], env=self.environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    ) as vision:
      assert vision.stdout is not None and vision.stdin is not None
      self.peer_read(vision, 'READY')
      try:
        self.publish(vision)
      finally:
        if vision.poll() is None:
          vision.stdin.write('stop\n')
          vision.stdin.flush()
          self.peer_read(vision, 'OK')
          assert vision.wait(timeout=10) == 0
    self.completed = True

  @staticmethod
  def peer_read(vision: subprocess.Popen[str], expected: str) -> None:
    assert vision.stdout is not None
    while True:
      line = vision.stdout.readline()
      assert line, vision.poll()
      if not line.startswith(('Starting listener', 'Stopping listener')):
        assert line.strip() == expected, line
        return

  def publish(self, vision: subprocess.Popen[str]) -> None:
    assert vision.stdin is not None
    publisher = messaging.PubMaster([name for name in SERVICES if name != 'selfdriveState'])
    selfdrive_publisher = None
    subscriptions = messaging.SubMaster(['uiDebug', 'bookmarkButton'])
    self.started.set()
    vision_frame = 0
    while not self.stop.is_set():
      vision.stdin.write(f'send {vision_frame}\n')
      vision.stdin.flush()
      self.peer_read(vision, 'OK')
      vision_frame += 1
      with self.lock:
        onroad, standstill, index, command, argument = self.onroad, self.standstill, self.command_index, self.command, self.argument
      if onroad and selfdrive_publisher is None:
        selfdrive_publisher = messaging.PubMaster(['selfdriveState'])
      elif not onroad:
        selfdrive_publisher = None
      points = [float(index * 3) for index in range(33)]
      values = {
        'pandaStates': [{'pandaType': 'dos', 'ignitionLine': onroad}],
        'deviceState': {'started': onroad, 'deviceType': 'pc', 'cpuTempC': [60.0, 61.0], 'networkType': 'wifi', 'networkStrength': 'good'},
        'carState': {
          'vEgo': 15.0 if onroad else 0.0,
          'vEgoCluster': 15.0 if onroad else 0.0,
          'vCruiseCluster': 80.0,
          'standstill': standstill,
          'gearShifter': 'drive',
        },
        'selfdriveState': {'enabled': False},
        'carControl': {'latActive': False},
        'driverStateV2': {
          side: {'faceOrientation': [0.0, 0.0, 0.0], 'faceOrientationStd': [0.1, 0.1, 0.1], 'facePosition': [0.0, 0.0]}
          for side in ['leftDriverData', 'rightDriverData']
        },
        'controlsState': {'lateralControlState': {'torqueState': {}}},
        'liveParameters': {'steerRatio': 13.4},
        'longitudinalPlan': {'myDrivingMode': 3, 'speeds': [15.0] * 17, 'accels': [0.0] * 17},
        'modelV2': {
          'position': {'x': points, 'y': [0.0] * 33, 'z': [0.0] * 33},
          'velocity': {'x': [15.0] * 33},
          'laneLines': [{'x': points, 'y': [offset] * 33, 'z': [0.0] * 33} for offset in [-4.0, -2.0, 2.0, 4.0]],
          'laneLineProbs': [0.1, 0.9, 0.9, 0.1],
          'roadEdges': [{'x': points, 'y': [offset] * 33, 'z': [0.0] * 33} for offset in [-5.0, 5.0]],
          'roadEdgeStds': [1.0, 1.0],
        },
        'carrotMan': {'carrotCmdIndex': index, 'carrotCmd': command, 'carrotArg': argument, 'szPosRoadName': 'Runtime fixture road'},
        'roadCameraState': {'sensor': 'unknown'},
        'wideRoadCameraState': {'sensor': 'unknown', 'exposureValPercent': 20.0},
      }
      for name in SERVICES:
        if name == 'selfdriveState' and selfdrive_publisher is None:
          continue
        message = log.Event.new_message()
        message.valid = True
        message.logMonoTime = time.monotonic_ns()
        if name in ['pandaStates', 'onroadEvents', 'customReservedRawData0']:
          message.init(name, 1 if name == 'pandaStates' else 0)
        else:
          message.init(name)
        if name in values:
          message.from_dict({name: values[name]})
        (selfdrive_publisher if name == 'selfdriveState' else publisher).send(name, message)
      subscriptions.update(0)
      if subscriptions.updated['uiDebug']:
        assert subscriptions.valid['uiDebug'] and subscriptions.logMonoTime['uiDebug'] > 0
        self.debug_records.append(json.dumps(subscriptions['uiDebug'].to_dict()))
      if subscriptions.updated['bookmarkButton']:
        assert subscriptions.valid['bookmarkButton'] and subscriptions.logMonoTime['bookmarkButton'] > 0
        self.bookmark_received.set()
      self.stop.wait(0.05)

  def set_onroad(self, value: bool) -> None:
    with self.lock:
      self.onroad = value

  def record_command(self, argument: str) -> None:
    with self.lock:
      self.command_index += 1
      self.command = 'RECORD'
      self.argument = argument
