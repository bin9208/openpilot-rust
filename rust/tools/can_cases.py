"""Source-backed codec vectors and parser lifecycle scenarios for the complete DBC catalog."""

from pathlib import Path
import random

from can_source import ROOT, load


def databases(staging):
  load()
  from opendbc.car.values import PLATFORMS
  from opendbc.dbc.generator.generator import generate_all
  generated = generate_all()
  staging.mkdir(parents=True, exist_ok=True)
  names = sorted({name for p in PLATFORMS.values() for name in p.config.dbc_dict.values() if name})
  paths = {}
  missing = {}
  for name in names:
    path = ROOT / 'opendbc_repo/opendbc/dbc' / (name + '.dbc')
    if not path.exists():
      if name not in generated:
        missing[name] = str(path)
        continue
      path = staging / (name + '.dbc')
      path.write_text(generated[name])
    paths[name] = str(path)
  paths['test'] = str(ROOT / 'opendbc_repo/opendbc/can/tests/test.dbc')
  paths['fca_giorgio'] = str(ROOT / 'opendbc_repo/opendbc/dbc/fca_giorgio.dbc')
  identities = {name: dict(brand=p.__module__.split('.')[-2], dbcs=list(p.config.dbc_dict.values())) for name, p in PLATFORMS.items()}
  return paths, identities, missing


def codec(path):
  DBC, _, _ = load()
  dbc = DBC(path)
  rng = random.Random(177)
  steps = [dict(op='define')]
  for address, message in dbc.msgs.items():
    steps.append(dict(op='pack', address=address, values=[], rx_counter=3))
    for index in range(8):
      values = []
      for signal in message.sigs.values():
        raw = rng.randrange(1 << signal.size)
        if signal.is_signed and raw >= (1 << (signal.size - 1)):
          raw -= 1 << signal.size
        if index % 4 == 0:
          raw = index // 4 - 1
        values.append([signal.name, raw * signal.factor + signal.offset])
      steps.append(dict(op='pack', address=address, values=values, rx_counter=None))
    steps.append(dict(op='pack', address=address, values=[], rx_counter=None))
  steps.extend([dict(op='pack', address=0xFFFFFFFF, values=[], rx_counter=None)])
  return dict(path=path, bus=0, now=1_000_000_000, messages=[], steps=steps)


def lifecycle(path):
  DBC, CANPacker, _ = load()
  dbc = DBC(path)
  messages = [m for m in dbc.msgs.values() if m.size >= 2 and m.sigs]
  if not messages:
    return None
  # Exercise the first checksum/counter-bearing message when available.
  message = next((m for m in messages if any(s.type for s in m.sigs.values())), messages[0])
  packer = CANPacker(path)
  steps = []
  def update(nanos, frames, checks=1):
    steps.append(dict(op='update', packets=[dict(mono_time=nanos, frames=frames)], checks=checks))
  def frame(data, bus=0, address=message.address):
    return dict(address=address, data=list(data), bus=bus)
  update(1_000_000_000, [], 1)
  steps.append(dict(op='ready', enabled=True))
  for index in range(16):
    try:
      data = packer.pack(message.address, {})
    except TypeError:
      # The MLB source error is fully covered by codec vectors; it cannot produce a valid lifecycle frame.
      return None
    update(1_010_000_000 + index * 10_000_000, [frame(data)])
  bad = bytearray(data)
  bad[-1] ^= 0xFF
  update(1_180_000_000, [frame(bad)])
  update(1_190_000_000, [frame(data, bus=1)])
  update(1_200_000_000, [frame(data, address=0x7FFFFFFF)])
  update(1_210_000_000, [frame(bytes(65))])
  update(1_220_000_000, [frame(b'')])
  update(1_230_000_000, [frame(data)] * 6)
  update(3_230_000_001, [], 5)
  update(3_240_000_000, [frame(packer.pack(message.address, {}))], 3)
  steps.append(dict(op='update', packets=[], checks=2))
  steps.append(dict(op='add', name=message.name, frequency=1, ignore_counter=False, now=4_000_000_000))
  return dict(path=path, bus=0, now=1_000_000_000, messages=[[message.name, 100, False]], steps=steps)


def learning(path):
  DBC, CANPacker, _ = load()
  message = DBC(path).msgs[245]
  packer = CANPacker(path)
  steps = []
  for index in range(510):
    data = packer.pack(message.address, {})
    steps.append(dict(op='update', packets=[dict(mono_time=1_000_000_000 + index * 10_000_000,
                      frames=[dict(address=message.address, data=list(data), bus=0)])], checks=1))
  # Learning, buffer rollover, stale observations and counter overflow recovery.
  return dict(path=path, bus=0, now=1_000_000_000, messages=[[message.name, None, False]], steps=steps)


def ignored(path):
  return dict(path=path, bus=0, now=1_000_000_000, messages=[], ignored_messages=['CAN_FD_MESSAGE'], steps=[
    dict(op='update', packets=[dict(mono_time=10_000_000_000, frames=[])], checks=5),
    dict(op='update', packets=[], checks=2)])


def known_fca_rejection(path):
  vectors = [[0x7b, 0x30, 0, 0xf8], [0x7b, 0x10, 1, 0x90], [0x7b, 0xf0, 2, 0x6e]]
  return dict(path=path, bus=0, now=1_000_000_000, messages=[['EPS_3', 100, False]], steps=[
    dict(op='update', packets=[dict(mono_time=1_000_000_000 + index * 10_000_000,
         frames=[dict(address=0x122, data=data, bus=0)])], checks=1) for index, data in enumerate(vectors)])


def lazy(path):
  _, CANPacker, _ = load()
  data = list(CANPacker(path).pack(245, {'SIGNED': -17}))
  update = dict(op='update', packets=[dict(mono_time=1_000_000_000, frames=[dict(address=245, data=data, bus=0)])], checks=1)
  return dict(path=path, bus=0, now=1_000_000_000, messages=[], steps=[update,
              dict(op='lazy', name='CAN_FD_MESSAGE', signal='SIGNED', now=1_000_000_000), update,
              dict(op='lazy', name='CAN_FD_MESSAGE', signal='SIGNED', now=1_000_000_000)])
