"""Replay the checked ARS510 inputs through pinned source on this CPU's NumPy.

Only validation reads this reference. The production provider is never called to
construct expectations; private SVD/filter bits come from the original source.
"""
import argparse
from collections import deque
from dataclasses import asdict, is_dataclass
import ctypes
import hashlib
import json
import math
from pathlib import Path
import platform
import subprocess

import numpy as np
import zstandard

from card_toyota_final_source import PIN, install
from can_source import ROOT, load
from radarcan_base_source import data as base_data, track_fields


def encode(value):
  if is_dataclass(value):
    return encode(asdict(value))
  if isinstance(value, float) and not math.isfinite(value):
    return 'NaN' if math.isnan(value) else 'Infinity' if value > 0 else '-Infinity'
  if isinstance(value, bytes):
    return list(value)
  if isinstance(value, set):
    return [encode(item) for item in sorted(value)]
  if isinstance(value, (tuple, list, deque)):
    return [encode(item) for item in value]
  if isinstance(value, dict):
    return {('fused' if str(key) == '_fused' else 'range_est' if str(key) == '_range_est' else str(key)): encode(item)
            for key, item in value.items() if not callable(item)}
  if hasattr(value, 'to_dict'):
    return encode(value.to_dict())
  if hasattr(value, 'tolist'):
    return encode(value.tolist())
  if hasattr(value, '__dict__'):
    return encode(vars(value))
  return value


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  args = parser.parse_args()
  args.output.parent.mkdir(parents=True, exist_ok=True)
  assert np.__version__ == '2.5.3'
  manifest = json.loads((args.numerics / 'manifest.json').read_text())
  library = manifest['library']
  source_library = Path(np.__file__).resolve().parent.parent / 'numpy.libs' / library
  digest = hashlib.sha256(source_library.read_bytes()).hexdigest()
  assert any(item['name'] == library and item['sha256'] == digest for item in manifest['files']), 'source and native NumPy provider must match'
  get_core = ctypes.CDLL(str(source_library)).scipy_openblas_get_corename64_
  get_core.restype = ctypes.c_char_p
  cpu_kernel = get_core().decode('ascii')
  closure = install(args.output.parent / 'ars510-source')
  # These shared dependencies are retained unchanged; reject source drift.
  for path in ('opendbc_repo/opendbc/car/radar_tracks.py', 'opendbc_repo/opendbc/car/radar_lead_filter.py',
               'opendbc_repo/opendbc/car/interfaces.py', 'openpilot/common/filter_simple.py'):
    original = subprocess.check_output(['git', 'show', f'{PIN}:{path}'], cwd=ROOT)
    assert (ROOT / path).read_bytes() == original, path
    closure['files'][path] = hashlib.sha256(original).hexdigest()
  load()
  from opendbc.car import structs
  from opendbc.car.toyota.ars510_carrot import Ars510RadarInterface as Receiver
  from opendbc.car.toyota import ars510_carrot
  warnings = []
  ars510_carrot.carlog.warning = warnings.append
  base_names = ('frame', 'v_ego_hist', 'a_ego_hist', 'v_ego', 'a_ego', 'last_timestamp', 'dt', 'init_samples', 'init_done')
  excluded = {*base_names, 'CP', 'rcp', 'pts', 'tracks'}

  def new():
    return Receiver(structs.CarParams(brand='toyota', carFingerprint='TOYOTA_RAV4_TSS2_2022',
      flags=64, radarUnavailable=True, radarDelay=.2, radarTimeStep=0.))

  def snapshot(model):
    own = {key: value for key, value in vars(model).items() if key not in excluded}
    base = {key: getattr(model, key) for key in base_names}
    base.update(pts={str(key): value.to_dict() for key, value in model.pts.items()}, pts_order=list(model.pts),
      tracks={str(key): track_fields(value) for key, value in model.tracks.items()}, tracks_order=list(model.tracks),
      history_maxlen=model.v_ego_hist.maxlen)
    return encode(dict(receiver=own, base=base, diagnostics=model.diagnostics))

  def data(value):
    if value is None:
      return None
    result = base_data(value)
    if value.hasDisplayData:
      result['hasDisplayData'] = True
    if len(value.displayPoints):
      result['displayPoints'] = [point.to_dict() for point in value.displayPoints]
    return encode(result)

  source = ROOT / 'rust/crates/radarcan/tests/fixtures/ars510-receiver-final.jsonl.zst'
  count = 0
  with source.open('rb') as raw, zstandard.ZstdDecompressor().stream_reader(raw) as reader, \
       args.output.open('wb') as target, zstandard.ZstdCompressor(level=3).stream_writer(target) as writer:
    import io
    for line in io.TextIOWrapper(reader):
      row = json.loads(line)
      warnings.clear()
      if row.get('op') == 'new':
        model = new()
      else:
        event = row['input']
        packets = [(stamp, [(address, bytes(body), bus) for address, body, bus in frames])
                   for stamp, frames in event.get('packets', [])]
        result = error = None
        try:
          match event['op']:
            case 'rebuild':
              previous = model
              model = new()
              model.recover_from_input_fault(previous)
            case 'guard_generation':
              model.decoder._guard_gen[1] = event['value']
              model.display_decoder._guard_gen[1] = event['value']
            case 'update':
              result = model.update(packets)
            case 'update_carrot':
              result = model.update_carrot(float(event['speed']), float(event['accel']), event['time'], packets)
            case _:
              raise AssertionError(event['op'])
        except Exception as failure:
          error = dict(kind=type(failure).__name__, message=str(failure))
        row.update(result=data(result), error=error)
        count += 1
      row.update(expected=snapshot(model), warnings=warnings.copy())
      writer.write(json.dumps(row, separators=(',', ':')).encode() + b'\n')
  assert count == 2697, 'the existing checked action corpus must be preserved'
  report = dict(source_commit=PIN, source_files=closure['files'], numpy=np.__version__, machine=platform.machine(),
    numpy_library_sha256=digest, numpy_cpu_kernel=cpu_kernel, input_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), actions=count,
    reference_sha256=hashlib.sha256(args.output.read_bytes()).hexdigest(),
    boundary='checked input actions only; complete original source state/results; no native-derived expectations')
  args.output.with_suffix('.manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
