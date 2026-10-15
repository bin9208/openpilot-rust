"""Temporary offline adapter comparison against captured actual final radarcan.py IPC.

All control/display/validity payload fields compare exactly, including Float32
bit patterns. Publication wall-clock headers are reported separately: offline
replay intentionally stamps its recorded timeline, not the original process clock.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
from openpilot.cereal import log


def payload(raw):
  with log.Event.from_bytes(bytes.fromhex(raw)) as event:
    assert event.which() == 'liveTracks'
    value = dict(valid=event.valid, liveTracks=event.liveTracks.to_dict())
    timestamp = event.logMonoTime
  def bits(value):
    if isinstance(value, float): return dict(float32_bits=struct.pack('<f', value).hex())
    if isinstance(value, list): return [bits(item) for item in value]
    if isinstance(value, dict): return {key: bits(item) for key, item in value.items()}
    return value
  return bits(value), timestamp


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--captures', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--runner', type=Path)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--dbc', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
  cases = []
  for directory in sorted(args.captures.glob('bosch-runtime-*')):
    if not (directory / 'case.json').is_file(): continue
    case = json.loads((directory / 'case.json').read_text())
    assert json.loads((directory / 'result.json').read_text())['status'] == 'pass'
    request = dict(car_params=str((directory / 'native/CarParams.bin').resolve()),
                   dbc=str(args.dbc.resolve()), numerics=str(args.numerics.resolve()),
                   settings={key:int(value) for key,value in case['params'].items()}, flipped=bool(case['flip']),
                   scene=json.loads((directory / 'context-inputs.json').read_text()),
                   steps=json.loads((directory / 'inputs.json').read_text()))
    (args.output / (directory.name+'-input.json')).write_text(json.dumps(request)+'\n')
    command = ([str(args.runner.resolve())] if args.runner else []) + [str(args.binary.resolve())]
    environment = dict(os.environ, OPENBLAS_NUM_THREADS='1', OMP_NUM_THREADS='1')
    result = subprocess.run(command,input=json.dumps(request),text=True,capture_output=True,env=environment,check=True)
    (args.output / (directory.name+'-native.json')).write_text(result.stdout)
    (args.output / (directory.name+'-native.log')).write_text(result.stderr)
    actual_rows = json.loads(result.stdout)
    errors = [(row['step'],row['errors']) for row in actual_rows if row['errors']]
    assert not errors, (directory.name, errors)
    actual = [raw for row in actual_rows for raw in row['publications']]
    expected = json.loads((directory / 'source/publications.json').read_text())
    assert len(actual) == len(expected), (directory.name, 'publication count',len(actual),len(expected))
    header_deltas = []
    for index,(native,source) in enumerate(zip(actual,expected,strict=True)):
      native_payload, native_ns = payload(native)
      source_payload, source_ns = payload(source['raw'])
      assert native_payload == source_payload, (directory.name,index,source_payload,native_payload)
      header_deltas.append(native_ns-source_ns)
    cases.append(dict(name=directory.name,steps=len(actual_rows),publications=len(actual),float32_bits_exact=True,
                      source_capture_sha256=hashlib.sha256((directory/'source/publications.json').read_bytes()).hexdigest(),
                      source_inputs_sha256=hashlib.sha256((directory/'inputs.json').read_bytes()).hexdigest(),
                      header_policy='recorded-timeline offline header; original wall-clock header kept separately',
                      header_delta_ns_min=min(header_deltas),header_delta_ns_max=max(header_deltas)))
  assert len(cases)==4, len(cases)
  receipt=dict(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3',cases=cases,
               total_steps=sum(case['steps'] for case in cases),total_publications=sum(case['publications'] for case in cases),
               source='actual full final radarcan.py+lateral.py real IPC captures; source source-path fingerprints owned by vehicle gate')
  (args.output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))


if __name__ == '__main__': main()
