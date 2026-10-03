import ast
import os
import sys
import types
from card_firmware_query_source import Input, Wire
from card_firmware_source import setup


def firmware(version):
  return dict(ecu=str(version.ecu), fw_version=list(version.fwVersion), address=version.address, response_address=version.responseAddress,
              request=[list(item) for item in version.request], brand=version.brand, bus=version.bus, logging=version.logging,
              obd_multiplexing=version.obdMultiplexing, sub_address=version.subAddress)


def cached(case):
  from opendbc.car import structs
  if case['cache'] is None:
    return None
  cache = case['cache']
  fw = [structs.CarParams.CarFw.new_message(ecu=version['ecu'], fwVersion=bytes(version['fw_version']), address=version['address'],
          subAddress=version.get('sub_address', 0), brand=version['brand']) for version in cache['firmware']]
  return structs.CarParams.new_message(brand=cache['brand'], carVin=cache['vin'], carFw=fw)


def trace(case: dict) -> dict:
  setup()
  from opendbc.car import structs, uds, isotp_parallel_query, ecu_addrs
  from opendbc.car.values import BRANDS
  class Interface:
    def __init__(self, cp):
      self.CP = cp
    @staticmethod
    def get_params(candidate, *args, **kwargs):
      return structs.CarParams.new_message(carFingerprint=candidate)
  for brand in BRANDS:
    module = brand.__module__.rsplit('.', 1)[0] + '.interface'
    boundary = types.ModuleType(module)
    boundary.CarInterface = Interface
    sys.modules[module] = boundary
  from opendbc.car import car_helpers
  from opendbc.car.can_definitions import CanData
  wire = Wire(case['io'])
  def receive(wait_for_one=False):
    packets = wire.receive(wait_for_one)
    passive = [CanData(frame['address'], bytes(frame['data']), frame['bus']) for frame in case['passive']]
    return packets + [passive] * 202
  timer = types.SimpleNamespace(monotonic=wire.clock, sleep=wire.delays.append)
  car_helpers.time = timer
  uds.time = timer
  isotp_parallel_query.time = timer
  ecu_addrs.time = timer
  records = []
  logs = []
  class Logger:
    def warning(self, *args):
      logs.append(['warning', args[0] % args[1:] if len(args) > 1 else args[0]])
    def error(self, value):
      logs.append(['error', value])
      if isinstance(value, dict) and value.get('event') == 'fingerprinted':
        records.append(value)
  class Settings:
    def get(self, key):
      return case['selected'] if key == 'CarSelected3' else None
    def put(self, key, value):
      pass
  car_helpers.carlog = Logger()
  car_helpers.Params = Settings
  original = car_helpers.interfaces
  car_helpers.interfaces = {name: Interface for name in original}
  keys = ['FINGERPRINT', 'SKIP_FW_QUERY', 'DISABLE_FW_CACHE']
  previous = {key: os.environ.get(key) for key in keys}
  try:
    for key, value in zip(keys, [case['fixed'], '1' if case['skip'] else '', '1' if case['disable_cache'] else ''], strict=True):
      os.environ[key] = value
    result = car_helpers.get_car(receive, wire.send, wire.obd.append, False, True, case['pandas'], cached(case))
  finally:
    car_helpers.interfaces = original
    for key, value in previous.items():
      if value is None:
        os.environ.pop(key, None)
      else:
        os.environ[key] = value
  cp = result.CP
  record = records[-1]
  observed = ast.literal_eval(record['fingerprints'])
  value = dict(candidate=cp.carFingerprint, observed=[[bus, [[addr, length] for addr, length in frames.items()]] for bus, frames in observed.items()],
               vin=cp.carVin, firmware=[firmware(version) for version in cp.carFw], source=str(cp.fingerprintSource), exact_match=not cp.fuzzyFingerprint,
               cached=record['cached'], vin_rx_address=None if record['vin_rx_addr'] == -1 else record['vin_rx_addr'],
               vin_rx_bus=None if record['vin_rx_bus'] == -1 else record['vin_rx_bus'], ecu_responses=[list(item) for item in record['ecu_responses']],
               fw_query_time=record['fw_query_time'], packets=202)
  for level, message in logs:
    if isinstance(message, dict) and message.get('event') == 'fingerprinted':
      message['source'] = int(message['source'])
      message['ecu_responses'] = [list(item) for item in message['ecu_responses']]
  return dict(result=value, io=wire.snapshot(), logs=logs)
