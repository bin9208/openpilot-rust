from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
from card_qa.ci import ROOT, TOOLS, hashes, require_space, run, source_command
from stage_card_corpus import BRANDS

SHARED = ('core', 'startup', 'startup_params', 'cruise', 'vehicle', 'mini_vehicle', 'isotp', 'query',
  'firmware', 'firmware_query', 'identification', 'diagnostic', 'xiaoge', 'toyota_secoc')
IPC_CANDIDATES = ('COMMA_BODY', 'MOCK', 'GENESIS_G70', 'TESLA_MODEL_3', 'TESLA_MODEL_Y', 'MAZDA_CX5_2022',
  'NISSAN_XTRAIL', 'CHRYSLER_PACIFICA_2018', 'RIVIAN_R1_GEN1', 'FORD_F_150_MK14', 'FORD_MAVERICK_MK1',
  'SUBARU_ASCENT', 'TOYOTA_PRIUS', 'CHEVROLET_VOLT', 'HONDA_CRV_5G', 'VOLKSWAGEN_PASSAT_NMS',
  'VOLKSWAGEN_ID4_MK1', 'VOLKSWAGEN_ID4_MK2')


def reference(arguments: argparse.Namespace) -> None:
  binary, dbc, numerics, output = arguments.binaries, arguments.dbc, arguments.numerics, arguments.evidence
  commands = output / 'commands'
  for name in SHARED:
    require_space(output, 1024**3)
    args = ['--binary', str(binary / f'{name}_trace'), '--evidence', str(output / name)]
    run(source_command(TOOLS / f'check_card_{name}.py', args, dbc), commands, name)
  for name in ('git_source', 'carlog'):
    require_space(output, 1024**3)
    run([sys.executable, str(TOOLS / f'check_card_{name}.py'), '--binary', str(binary / f'{name}_trace'),
      '--output', str(output / name)], commands, name)
  require_space(output, 1024**3)
  run(source_command(TOOLS / 'check_card_common.py', ['--binary', str(binary / 'common_trace'),
    '--numerics', str(numerics), '--evidence', str(output / 'common')], dbc), commands, 'common')
  for brand in BRANDS:
    require_space(output, 4 * 1024**3)
    args = ['--binary', str(binary / f'{brand}_trace'), '--numerics', str(numerics), '--evidence', str(output / brand)]
    if brand in ('subaru', 'toyota', 'honda'):
      args += ['--dbc', str(dbc)]
    run(source_command(TOOLS / f'check_card_{brand}.py', args, dbc), commands, brand)
  require_space(output, 1024**3)
  run(source_command(TOOLS / 'check_card_psa_source.py', ['--binary', str(binary / 'psa_boundary'),
    '--evidence', str(output / 'psa')], dbc), commands, 'psa-boundaries')
  require_space(output, 1024**3)
  run([sys.executable, str(TOOLS / 'check_card_factory.py'), '--binary', str(binary / 'factory_trace'),
    '--binding', str(arguments.binding), '--numerics', str(numerics), '--output', str(output / 'factory')], commands, 'factory')
  require_space(output, 2 * 1024**3)
  run(source_command(TOOLS / 'check_can.py', ['--binary', str(binary / 'can_trace'), '--staging', str(output / 'can-assets'),
    '--evidence', str(output / 'can')], dbc), commands, 'can')
  require_space(output, 1024**3)
  run(source_command(TOOLS / 'check_can_checksums.py', ['--binary', str(binary / 'checksum_trace'),
    '--evidence', str(output / 'checksums')], dbc), commands, 'checksums')


def ipc(arguments: argparse.Namespace) -> None:
  output = arguments.evidence
  runtime_root = output / 'runtime-root'
  assets = runtime_root / 'opendbc_repo/opendbc'
  assets.mkdir(parents=True)
  (assets / 'dbc').symlink_to(arguments.dbc, target_is_directory=True)
  (assets / 'car').symlink_to(ROOT / 'opendbc_repo/opendbc/car', target_is_directory=True)
  args = ['--binary', str(arguments.daemon), '--numerics', str(arguments.numerics), '--binding', str(arguments.binding),
    '--runtime-root', str(runtime_root), '--corpus-root', str(arguments.corpus), '--evidence', str(output / 'pumped')]
  for candidate in IPC_CANDIDATES:
    args += ['--candidate', candidate]
  run([sys.executable, str(TOOLS / 'check_card_runtime_pumped.py'), *args], output / 'commands', 'pumped')


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--phase', choices=('reference', 'ipc'), required=True)
  for name in ('binaries', 'daemon', 'numerics', 'binding', 'dbc', 'corpus', 'evidence'):
    parser.add_argument('--' + name, type=Path, required=True)
  arguments = parser.parse_args()
  for name in ('binaries', 'daemon', 'numerics', 'binding', 'dbc', 'corpus', 'evidence'):
    setattr(arguments, name, getattr(arguments, name).resolve())
  require_space(arguments.evidence, 2 * 1024**3)
  arguments.evidence.mkdir(parents=True)
  sources = list((ROOT / 'rust/crates/card').rglob('*.rs')) + list((ROOT / 'rust/crates/can').rglob('*.rs'))
  sources += list(TOOLS.glob('*card*.py')) + list((TOOLS / 'card_qa').rglob('*.py')) + list(TOOLS.glob('*can*.py'))
  binaries = [arguments.daemon, *arguments.binaries.glob('*_trace'), arguments.binaries / 'psa_boundary']
  receipt = {'phase': arguments.phase, 'source_sha256': hashes(sources), 'binary_sha256': {
    str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries if path.is_file()}, 'argv': sys.argv}
  (arguments.evidence / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
  if arguments.phase == 'reference':
    reference(arguments)
  else:
    ipc(arguments)
  (arguments.evidence / 'result.json').write_text(json.dumps({'status': 'pass', 'phase': arguments.phase}) + '\n')


if __name__ == '__main__':
  main()
