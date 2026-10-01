import struct
import json
from pathlib import Path
from openpilot.system.ubloxd import binary_struct as bs
from openpilot.system.ubloxd.ubx import Ubx
from openpilot.system.ubloxd.gps import Gps
from openpilot.system.ubloxd.glonass import Glonass


class Writer:
  def __init__(self):
    self.bits = []

  def bitfield(self, value, width):
    self.bits.extend((value >> i) & 1 for i in reversed(range(width)))

  def raw(self, value):
    while len(self.bits) % 8:
      self.bits.append(0)
    for byte in value:
      self.bitfield(byte, 8)

  def finish(self):
    self.raw(b'')
    return bytes(sum(self.bits[i + j] << (7 - j) for j in range(8)) for i in range(0, len(self.bits), 8))


def fields(writer, schema, values):
  for name, spec in schema.__binary_fields__:
    kind = bs._field_type_from_spec(spec)
    value = values.get(name, 0)
    if isinstance(kind, bs.ConstType):
      value, kind = kind.expected, kind.base_type
    if isinstance(kind, bs.EnumType):
      kind = kind.base_type
    if isinstance(kind, bs.IntType):
      writer.raw(struct.pack(bs._int_format(kind), value))
    elif isinstance(kind, bs.FloatType):
      writer.raw(struct.pack(bs._float_format(kind), value))
    elif isinstance(kind, bs.BitsType):
      writer.bitfield(value, kind.bits)
    elif isinstance(kind, bs.BytesType):
      writer.raw(value if isinstance(value, bytes) else bytes(kind.size))
    elif isinstance(kind, bs.ArrayType):
      for item in values.get(name, []):
        fields(writer, kind.element_type, item)
    else:
      raise TypeError(f'unsupported fixture {schema}.{name}')


def payload(schema, **values):
  writer = Writer()
  fields(writer, schema, values)
  return writer.finish()


def frame(kind, data):
  body = kind.to_bytes(2, 'big') + len(data).to_bytes(2, 'little') + data
  a = b = 0
  for value in body:
    a = (a + value) & 255
    b = (b + a) & 255
  return b'\xb5\x62' + body + bytes([a, b])


def gps(number, *, iode=43, sv=9, week=852, tow=100000, **changes):
  defaults = {
    1: {'week_no': week, 'sv_health': 3, 't_gd': -7, 'iodc_lsb': iode, 't_oc': 24, 'af_2': -10, 'af_1': -32760, 'af_0_sign': 1, 'af_0_value': 15432},
    2: {'iode': iode, 'c_rs': -323, 'delta_n': -14, 'm_0': -2147483000, 'c_uc': 223, 'e': -12345678, 'c_us': -89, 'sqrt_a': 3123456789, 't_oe': 0},
    3: {
      'c_ic': -235,
      'omega_0': -123456789,
      'c_is': 178,
      'i_0': 2147483000,
      'c_rc': 383,
      'omega': -99876543,
      'omega_dot_sign': 1,
      'omega_dot_value': 12345,
      'iode': iode,
      'idot_sign': 1,
      'idot_value': 2345,
    },
  }
  data = b'\x8b\x00\x00' + payload(Gps.How, tow_count=tow, subframe_id=number)
  data += payload(getattr(Gps, f'Subframe{number}'), **(defaults[number] | changes))
  assert len(data) == 30
  words = [int.from_bytes(data[i : i + 3], 'big') << 6 for i in range(0, 30, 3)]
  return frame(0x0213, bytes([0, sv, 0, 0, 10, 0, 2, 0]) + struct.pack('<10I', *words))


def glonass(number, *, sv=7, frequency=5, superframe=123, idle=0, **changes):
  defaults = {
    1: {'p1': 2, 't_k': 2345, 'x_vel_sign': 1, 'x_vel_value': 2222, 'x_accel_sign': 1, 'x_accel_value': 6, 'x_sign': 1, 'x_value': 34567890},
    2: {'b_n': 5, 'p2': 1, 't_b': 69, 'y_vel_value': 98765, 'y_accel_value': 7, 'y_value': 12345678},
    3: {
      'p3': 1,
      'gamma_n_sign': 1,
      'gamma_n_value': 457,
      'l_n': 1,
      'z_vel_sign': 1,
      'z_vel_value': 1111,
      'z_accel_value': 15,
      'z_sign': 1,
      'z_value': 54321000,
    },
    4: {'tau_n_sign': 1, 'tau_n_value': 654321, 'delta_tau_n_sign': 1, 'delta_tau_n_value': 13, 'e_n': 27, 'p4': 1, 'f_t': 2, 'n_t': 1234, 'n': 4, 'm': 2},
    5: {'n_a': 532, 'tau_c': 123456789, 'n_4': 6, 'tau_gps': 567890, 'l_n': 1},
  }
  writer = Writer()
  writer.bitfield(idle, 1)
  writer.bitfield(number, 4)
  fields(writer, getattr(Glonass, f'String{number}'), defaults[number] | changes)
  writer.bitfield(0, 19)
  writer.bitfield(superframe, 16)
  writer.bitfield(0, 8)
  writer.bitfield(4, 8)
  data = writer.finish()
  assert len(data) == 16
  words = [int.from_bytes(data[i : i + 4], 'big') for i in range(0, 16, 4)]
  return frame(0x0213, bytes([6, sv, 0, frequency, 4, 0, 2, 0]) + struct.pack('<4I', *words))


def chunks():
  nav = {
    'year': 2026,
    'month': 9,
    'day': 30,
    'hour': 23,
    'min': 59,
    'sec': 60,
    'nano': -123456789,
    'fix_type': 3,
    'flags': 165,
    'num_sv': 17,
    'lon': -1223456789,
    'lat': 372345678,
    'height': -34567,
    'h_msl': -12000,
    'h_acc': 1245,
    'v_acc': 3456,
    'vel_n': -12345,
    'vel_e': 16777217,
    'vel_d': 423,
    'g_speed': 4567,
    'head_mot': -4321098,
    's_acc': 345,
    'head_acc': 45678,
  }
  measurement = {
    'pr_mes': 20202020.234,
    'cp_mes': -123456789.34,
    'do_mes': -123.456,
    'gnss_id': 6,
    'sv_id': 7,
    'freq_id': 3,
    'lock_time': 65535,
    'cno': 43,
    'pr_stdev': 255,
    'cp_stdev': 130,
    'do_stdev': 30,
    'trk_stat': 13,
  }
  satellite = {'gnss_id': 0, 'sv_id': 4, 'cno': 33, 'elev': -21, 'azim': -123, 'pr_res': -78, 'flags': 0xFEDCBA98}
  frames = [
    frame(0x0107, payload(Ubx.NavPvt, **nav)),
    frame(0x0107, payload(Ubx.NavPvt, **(nav | {'year': 0, 'month': 0, 'day': 0, 'flags': 0}))),
    frame(
      0x0215,
      payload(Ubx.RxmRawx, rcv_tow=345678.234, week=2345, leap_s=18, num_meas=2, rec_stat=5, meas=[measurement, measurement | {'gnss_id': 0, 'trk_stat': 2}]),
    ),
    frame(0x0135, payload(Ubx.NavSat, itow=123456, num_svs=2, svs=[satellite, satellite | {'gnss_id': 6}])),
    frame(0x0A09, payload(Ubx.MonHw, noise_per_ms=123, agc_cnt=456, a_status=3, a_power=2, flags=12, jam_ind=88)),
  ]
  frames.extend(
    frame(0x0A0B, payload(Ubx.MonHw2, ofs_i=-128, mag_i=255, ofs_q=-27, mag_q=243, cfg_source=cfg, low_lev_cfg=0xFEDCBA98, post_status=0x12345678))
    for cfg in (102, 111, 112, 113, 255)
  )
  malformed = [
    frame(0x0107, b'\x00'),
    frame(0x0107, payload(Ubx.NavPvt, **(nav | {'num_sv': 255}))),
    frame(0x0215, payload(Ubx.RxmRawx, leap_s=-1)),
    frame(0x0215, payload(Ubx.RxmRawx, num_meas=1, meas=[measurement | {'gnss_id': 255}])),
    frame(0x0135, payload(Ubx.NavSat, num_svs=1, svs=[satellite | {'gnss_id': 255}])),
    frame(0x0A09, payload(Ubx.MonHw, a_status=255)),
    frame(0x0213, bytes([255, 1, 0, 0, 0, 0, 0, 0])),
    frame(0x0213, b'\x00'),
    frame(0x0213, bytes([0, 1, 0, 0, 10, 0, 0, 0])),
    frame(0xABCD, b'ignored'),
  ]
  items = []

  def add(data, time=100.0):
    items.append({'time': time, 'bytes': list(data)})

  add(b'garbage\xb5')
  add(b'\x62' + frames[0][2:])
  add(b'\xb5')
  add(frames[0][1:5])
  add(frames[0][5:])
  add(b'')
  bad = bytearray(frames[1])
  bad[-1] ^= 0x80
  add(bytes(bad) + b'garbage' + b''.join(frames))
  add(b''.join(malformed))
  add(b''.join(gps(i) for i in (2, 1, 3)))
  add(gps(1, iode=1) + gps(2, iode=2) + gps(3, iode=1))
  add(gps(3, week=853) + gps(2, week=853) + gps(1, week=853))
  for i in range(1, 6):
    add(glonass(i), 100 + 2 * i)
  for i in range(1, 4):
    add(glonass(i, superframe=0, frequency=6), 100 + 2 * i)
  add(glonass(4, superframe=0, frequency=6), 118)
  add(glonass(5, superframe=0, frequency=6), 120)
  for i in range(1, 5):
    add(glonass(i, superframe=0, frequency=7), 100 + 2 * i)
  add(glonass(5, superframe=0, frequency=7), 120.001)
  for i in range(1, 6):
    add(glonass(i, superframe=124, frequency=7), 300 + 2 * i)
  for i in range(1, 6):
    add(glonass(i, sv=255), 400 + 2 * i)
  add(glonass(5, sv=17), 412)
  add(glonass(1, idle=1))
  add(b'\xb5\x62\x01\x07\xff\xffincomplete')
  return items


if __name__ == '__main__':
  output = Path(__file__).resolve().parents[1] / 'crates/ublox/tests/data/decoder.json'
  output.parent.mkdir(parents=True, exist_ok=True)
  output.write_text(json.dumps(chunks()) + '\n')
