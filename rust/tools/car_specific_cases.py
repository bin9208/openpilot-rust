#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Imported by: PYTHONPATH=. python rust/tools/check_car_specific.py --help
# ──────────────────
from __future__ import annotations

import itertools
import random

import numpy as np

BRANDS = ('body', 'mock', 'ford', 'nissan', 'chrysler', 'honda', 'toyota', 'gm', 'volkswagen', 'hyundai', 'tesla', 'other')
BOOL_FIELDS = ('doorOpen', 'seatbeltUnlatched', 'espDisabled', 'espActive', 'stockFcw', 'stockAeb', 'brakeHoldActive',
               'parkingBrake', 'accFaulted', 'steeringPressed', 'brakePressed', 'standstill', 'gasPressed',
               'vehicleSensorsInvalid', 'invalidLkasSetting', 'lowSpeedAlert', 'buttonEnable', 'steerFaultTemporary', 'steerFaultPermanent')


def cp(brand='other', **changes):
  return {'brand': brand, 'minSteerSpeed': 11., 'minEnableSpeed': 3., 'pcmCruise': True,
          'openpilotLongitudinalControl': True, 'networkLocation': 'fwdCamera', **changes}


def state(**changes):
  cruise = {'enabled': False, 'available': True, 'standstill': False, 'nonAdaptive': False, **changes.pop('cruiseState', {})}
  return {'vEgo': 15., 'vCruise': 0., 'carrotCruise': 0, 'gearShifter': 'drive', 'cruiseState': cruise,
          'brake': 0., 'activateCruise': 0, 'softHoldActive': 0, 'buttonEvents': [],
          **dict.fromkeys(BOOL_FIELDS, False), **changes}


def around(value):
  middle = np.float32(value)
  return [float(np.nextafter(middle, np.float32(-np.inf))), float(middle), float(np.nextafter(middle, np.float32(np.inf)))]


def cases():
  rows = []
  scenario = ''

  def init(name, params):
    nonlocal scenario
    scenario = name
    rows.append({'scenario': scenario, 'operation': 'init', 'cp': params})

  def step(changes=None, *, control=None, operation='update', **options):
    rows.append({'scenario': scenario, 'operation': operation, 'current': state(**(changes or {})),
                 'control': {'enabled': False, 'actuators': {'accel': 0.}, **(control or {})}, **options})

  def parameter(key, value=None, directory=False):
    rows.append({'scenario': scenario, 'operation': 'parameter', 'key': key,
                 'bytes': None if value is None else list(value), 'directory': directory})

  for brand, pcm, long, network in itertools.product(BRANDS, (False, True), (False, True), ('fwdCamera', 'gateway')):
    init(f'{brand}-pcm{pcm}-long{long}-{network}', cp(brand, pcmCruise=pcm, openpilotLongitudinalControl=long, networkLocation=network))
    for field in BOOL_FIELDS:
      step({field: True})
      step()
    for gear in ('unknown', 'park', 'drive', 'neutral', 'reverse', 'sport', 'low', 'brake', 'eco', 'manumatic'):
      step({'gearShifter': gear, 'activateCruise': 1})
    for button in ('unknown', 'leftBlinker', 'rightBlinker', 'accelCruise', 'decelCruise', 'cancel', 'lkas', 'altButton2', 'mainCruise',
                   'setCruise', 'resumeCruise', 'gapAdjustCruise', 'lfaButton', 'paddleLeft', 'paddleRight'):
      for pressed in (False, True):
        step({'buttonEvents': [{'type': button, 'pressed': pressed}], 'gearShifter': 'park'})
    for activate in (1, 2, 0, -1, -2, -3, -3, 0, -3, 1, -1):
      step({'activateCruise': activate, 'softHoldActive': 2})
    for enabled, available, stopped, adaptive in itertools.product((False, True), repeat=4):
      step({'cruiseState': {'enabled': enabled, 'available': available, 'standstill': stopped, 'nonAdaptive': adaptive}})
    speeds = sum((around(value) for value in (41.38888888888889, .001, 3., 3.5, 5., 11., 11.5, 12., 13., 15.)), [])
    for speed in speeds:
      for accel in (-.1, 0., .3, float(np.nextafter(np.float32(.3), np.float32(-np.inf))), .31):
        step({'vEgo': speed, 'standstill': speed < .001, 'brake': 20., 'cruiseState': {'standstill': True}},
             control={'enabled': True, 'actuators': {'accel': accel}})
    step({'lowSpeedAlert': True, 'vEgo': 0., 'buttonEnable': True, 'activateCruise': 1,
          'buttonEvents': [{'type': 'cancel', 'pressed': True}, {'type': 'cancel', 'pressed': False}]})
    step({'vCruise': 30., 'carrotCruise': 1}, control={'enabled': True})

  for brand in ('chrysler', 'volkswagen', 'hyundai'):
    for minimum in (-1., 0., .4, .401, .402, 10., 11.):
      init(f'{brand}-hysteresis-{minimum}', cp(brand, minSteerSpeed=minimum))
      for offset in (.5, 1., 2., 4.):
        for speed in around(minimum + offset) + list(reversed(around(minimum + offset))):
          step({'vEgo': speed})

  for brand, minimum in itertools.product(('honda', 'toyota', 'gm', 'volkswagen'), (-1., 0., .001, 3.)):
    init(f'{brand}-engagement-minimum-{minimum}', cp(brand, minEnableSpeed=minimum))
    for speed in around(minimum) + around(minimum + .5) + around(minimum + 2.):
      step({'vEgo': speed}, control={'enabled': True, 'actuators': {'accel': .5}})
  init('button-enable-no-entry-category', cp(pcmCruise=False))
  for field in BOOL_FIELDS:
    step()
    step({field: True, 'activateCruise': 1})
  for network in ('fwdCamera', 'gateway'):
    init(f'gm-brake20-{network}', cp('gm', networkLocation=network))
    for brake, stopped in itertools.product(around(20.), (False, True)):
      step({'vEgo': 0., 'brake': brake, 'standstill': stopped})

  for pressed_at_start in (False, True):
    init(f'steer-silent-initial-{pressed_at_start}', cp())
    for _ in range(25):
      step({'steerFaultTemporary': True, 'steeringPressed': pressed_at_start})
    step()
    for _ in range(149):
      step()
    step({'steerFaultTemporary': True})
    step({'steerFaultTemporary': True, 'steeringPressed': True})
    step({'steerFaultTemporary': True, 'steeringPressed': True})
    step({'steerFaultTemporary': True, 'standstill': True})
    for _ in range(22):
      step({'steerFaultTemporary': True})
    step()
  for count in (148, 149, 150):
    init(f'steer-unpressed-{count}', cp())
    for _ in range(count):
      step()
    step({'steerFaultTemporary': True})

  init('mute-100-frame-read-order', cp())
  parameter('MuteSeatbelt', b'1')
  parameter('MuteDoor', b'1')
  for frame in range(1, 302):
    if frame == 101:
      parameter('MuteSeatbelt', b'0')
      parameter('MuteDoor', b'1\0')
    if frame == 201:
      parameter('MuteSeatbelt', directory=True)
      parameter('MuteDoor', directory=True)
    step({'doorOpen': True, 'seatbeltUnlatched': True})
  for key in ('MuteSeatbelt', 'MuteDoor'):
    parameter(key)
  init('explicit-frame-zero-settings', cp('body'))
  rows.append({'scenario': scenario, 'operation': 'update_params'})

  for confirmed in (None, b'0', b'1', b'1\0', b'true'):
    init(f'tesla-lkas-confirmed-{confirmed!r}', cp('tesla'))
    parameter('ExperimentalModeConfirmed', confirmed)
    parameter('ExperimentalMode', b'0')
    for pressed in (True, True, False, True, False, True):
      step({'buttonEvents': [{'type': 'lkas', 'pressed': pressed}]})
  init('tesla-write-error-and-retry-edge', cp('tesla'))
  parameter('ExperimentalModeConfirmed', b'1')
  parameter('ExperimentalMode', directory=True)
  step({'buttonEvents': [{'type': 'lkas', 'pressed': True}]})
  step()
  parameter('ExperimentalMode', b'0')
  step({'buttonEvents': [{'type': 'lkas', 'pressed': True}]})
  init('shutdown-write-error-and-latch', cp())
  parameter('DoShutdown', directory=True)
  for _ in range(3):
    step({'gearShifter': 'park', 'buttonEvents': [{'type': 'cancel', 'pressed': False}, {'type': 'cancel', 'pressed': True}]})
  parameter('DoShutdown')

  for pcm, allow, cancel in itertools.product((False, True), repeat=3):
    init(f'common-options-{pcm}-{allow}-{cancel}', cp())
    for enabled in (False, True, True, False):
      step({'cruiseState': {'enabled': enabled}, 'buttonEvents': [{'type': 'cancel', 'pressed': False}]},
           operation='common', pcm_enable=pcm, allow_enable=allow, allow_button_cancel=cancel)

  rng = random.Random(168)
  for brand in BRANDS:
    init(f'{brand}-seed168-adversarial', cp(brand, pcmCruise=rng.choice((False, True))))
    for _ in range(250):
      step({**{field: rng.random() < .2 for field in BOOL_FIELDS}, 'vEgo': rng.uniform(-1., 50.), 'vCruise': rng.choice((0., 10.)),
            'carrotCruise': rng.randrange(-2, 3), 'gearShifter': rng.choice(('drive', 'park', 'neutral', 'reverse')),
            'activateCruise': rng.randrange(-3, 4), 'softHoldActive': rng.randrange(-1, 3), 'brake': rng.uniform(0., 25.),
            'cruiseState': {key: rng.choice((False, True)) for key in ('enabled', 'available', 'standstill', 'nonAdaptive')},
            'buttonEvents': [{'type': rng.choice(('cancel', 'lkas', 'mainCruise')), 'pressed': rng.choice((False, True))} for _ in range(rng.randrange(4))]},
           control={'enabled': rng.choice((False, True)), 'actuators': {'accel': rng.uniform(-1., 1.)}})
  return rows
