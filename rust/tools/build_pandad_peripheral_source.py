import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
from typing import Final

ROOT: Final = Path(__file__).resolve().parents[2]


def main() -> None:
  parser = argparse.ArgumentParser(description='Compile the unchanged Panda peripheral policy against recorded I/O.')
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--json11-prefix', type=Path, required=True)
  parser.add_argument('--sanitize', action='store_true')
  args = parser.parse_args()
  output: Path = args.output.resolve()
  prefix: Path = args.json11_prefix.resolve()
  output.mkdir(parents=True, exist_ok=False)
  free = shutil.disk_usage(output).free
  if free < (25 * 1024**3 + 100 * 1024**2):
    raise OSError(f'Insufficient free space for source fixture: {free} bytes')
  source = ROOT / 'openpilot/selfdrive/pandad/pandad.cc'
  fixture = ROOT / 'rust/tools/pandad_peripheral_source.cc'
  text = source.read_text()
  body = 'void process_peripheral_state(' + text.split('void process_peripheral_state(', 1)[1].split('\nvoid log_panda_serial(', 1)[0]
  constants = '\n'.join(re.findall(r'^#define (?:MAX_IR_PANDA_VAL|CUTOFF_IL|SATURATE_IL) .+$', text, flags=re.MULTILINE))
  (output / 'pandad_peripheral_body.inc').write_text(constants + '\n\n' + body)
  binary = output / 'pandad-peripheral-source'
  command = ['clang++', '-std=c++17', '-O1', '-g', f'-I{ROOT / "openpilot"}', f'-I{output}', f'-I{prefix / "include"}',
             str(fixture), str(prefix / 'lib/libjson11.a'), '-o', str(binary)]
  if args.sanitize:
    command[1:1] = ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
  result = subprocess.run(command, text=True, capture_output=True, check=False)
  (output / 'build.json').write_text(json.dumps({'command': command, 'exit': result.returncode,
                                              'stdout': result.stdout, 'stderr': result.stderr}, indent=2) + '\n')
  result.check_returncode()
  inputs = [source, fixture, ROOT / 'openpilot/common/util.h']
  report = {'scope': 'unchanged process_peripheral_state body and original C++ FirstOrderFilter/map_val; recorded message/Params/hardware boundaries',
            'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
            'body_sha256': hashlib.sha256(body.encode()).hexdigest(), 'sanitize': args.sanitize,
            'sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}}
  (output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
