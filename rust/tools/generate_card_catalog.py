"""Extract static legacy fingerprints from unchanged source; never import this at runtime."""

import hashlib
import json
import math
from pathlib import Path
import sys
import types

from can_source import ROOT, load


def main():
  load()
  from opendbc.car.fingerprints import _FINGERPRINTS
  entries = [dict(name=name, versions=versions) for name, versions in _FINGERPRINTS.items()]
  target = ROOT / 'rust/crates/card/data'
  target.mkdir(parents=True, exist_ok=True)
  (target / 'fingerprints.json').write_text(json.dumps(entries, indent=2) + '\n')
  files = [ROOT / 'opendbc_repo/opendbc/car/fingerprints.py', ROOT / 'opendbc_repo/opendbc/car/car_helpers.py',
           ROOT / 'openpilot/selfdrive/car/openpilot_toggle.py']
  files += sorted((ROOT / 'opendbc_repo/opendbc/car').glob('*/fingerprints.py'))
  result = dict(source_commit='31d7306882218e9fecc44aba0d5c034f0d1ca188', legacy_count=len(entries),
                source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files})
  (target / 'provenance.json').write_text(json.dumps(result, indent=2) + '\n')
  print('Native legacy fingerprint catalog:', len(entries))
  progress = types.ModuleType('tqdm')
  progress.tqdm = lambda iterable, disable=True: iterable
  sys.modules['tqdm'] = progress
  from opendbc.car.fw_versions import FW_QUERY_CONFIGS, VERSIONS
  from opendbc.car.fw_query_definitions import ECU_NAME
  from opendbc.car.values import PLATFORMS
  from opendbc.car.selected_car import get_selected_car_platform
  from opendbc.car.hyundai.values import CANFD_CAR, EV_CAR, CANFD_FUZZY_WHITELIST
  def ecu_entry(key, versions):
    ecu, address, subaddress = key
    return dict(ecu=ECU_NAME[ecu], address=address, subaddress=subaddress, versions=[list(value) for value in versions])
  brands = []
  models = []
  for brand, config in FW_QUERY_CONFIGS.items():
    brands.append(dict(brand=brand, requests=[dict(request=[list(v) for v in r.request], response=[list(v) for v in r.response],
                 whitelist=[ECU_NAME[ecu] for ecu in r.whitelist_ecus], offset=r.rx_offset, bus=r.bus, logging=r.logging,
                 obd_multiplexing=r.obd_multiplexing) for r in config.requests],
                 nonessential=[(ECU_NAME[ecu], names) for ecu, names in config.non_essential_ecus.items()],
                 extra=[ecu_entry(key, []) for key in config.extra_ecus],
                 fuzzy=config.match_fw_to_car_fuzzy is not None))
    for model, versions in VERSIONS[brand].items():
      platform = PLATFORMS[model].config
      models.append(dict(name=str(model), brand=brand, firmware=[ecu_entry(key, values) for key, values in versions.items()],
                         wmis=sorted(getattr(platform, 'wmis', [])), chassis=sorted(getattr(platform, 'chassis_codes', [])),
                         lines=sorted(getattr(platform, 'lines', [])), years=sorted(getattr(platform, 'years', [])),
                         fuzzy_allowed=model not in (CANFD_CAR - EV_CAR - CANFD_FUZZY_WHITELIST)))
  selectable = []
  for platform in PLATFORMS.values():
    for document in platform.config.car_docs:
      selected = get_selected_car_platform(document.name)
      if selected is not None and not any(name == document.name for name, _ in selectable):
        selectable.append((document.name, str(selected)))
  (target / 'firmware.json').write_text(json.dumps(dict(brands=brands, models=models, selected=selectable), indent=2) + '\n')
  fw_files = [ROOT / f'opendbc_repo/opendbc/car/{name}.py' for name in ('fw_versions', 'fw_query_definitions', 'selected_car', 'values')]
  fw_files += sorted((ROOT / 'opendbc_repo/opendbc/car').glob('*/values.py'))
  fw_files += sorted((ROOT / 'opendbc_repo/opendbc/car').glob('*/fingerprints.py'))
  (target / 'firmware-provenance.json').write_text(json.dumps(dict(source_commit=result['source_commit'], runtime_python=False,
                    source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in fw_files}), indent=2) + '\n')
  print('Native firmware catalog:', len(brands), 'brands', len(models), 'models', len(selectable), 'manual choices')
  from opendbc.car import Bus
  from opendbc.car.interfaces import get_torque_params, TORQUE_NN_MODEL_PATH
  from opendbc.car.common.simple_kalman import get_kalman_gain
  import numpy as np
  torque = get_torque_params()
  platforms = []
  for candidate, platform in PLATFORMS.items():
    specs = platform.config.specs
    platforms.append(dict(candidate=str(candidate), brand=type(platform).__module__.split('.')[-2], mass=specs.mass, wheelbase=specs.wheelbase, steer_ratio=specs.steerRatio,
                   center_front_ratio=specs.centerToFrontRatio, min_enable_speed=specs.minEnableSpeed,
                   min_steer_speed=specs.minSteerSpeed, tire_stiffness_factor=specs.tireStiffnessFactor, flags=int(platform.config.flags),
                   dbc_pt=platform.config.dbc_dict.get(Bus.main, platform.config.dbc_dict.get(Bus.pt)), dbc_radar=platform.config.dbc_dict.get(Bus.radar),
                   torque={key: value if math.isfinite(value) else None for key, value in torque[candidate].items()} if candidate in torque else None))
  gain = get_kalman_gain(.01, np.array([[1., .01], [0., 1.]]), np.array([[1., 0.]]), np.array([[0., 0.], [0., 100.]]), .3)
  vehicle = dict(platforms=platforms, ff_files=[path.name for path in Path(TORQUE_NN_MODEL_PATH).iterdir()], speed_gain=[float(value) for value in gain[:, 0]])
  (target / 'vehicle.json').write_text(json.dumps(vehicle, indent=2) + '\n')
  vehicle_files = [ROOT / 'opendbc_repo/opendbc/car/interfaces.py', ROOT / 'opendbc_repo/opendbc/car/__init__.py',
                   ROOT / 'opendbc_repo/opendbc/car/common/simple_kalman.py']
  vehicle_files += sorted((ROOT / 'opendbc_repo/opendbc/car/torque_data').glob('*.toml'))
  (target / 'vehicle-provenance.json').write_text(json.dumps(dict(source_commit=result['source_commit'], runtime_python=False,
                   source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in vehicle_files}), indent=2) + '\n')
  print('Native vehicle baseline:', len(platforms), 'identities', len(vehicle['ff_files']), 'feedforward entries', vehicle['speed_gain'])


if __name__ == '__main__':
  main()
