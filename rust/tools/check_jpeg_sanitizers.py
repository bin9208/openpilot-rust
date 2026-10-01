#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target-dir', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  assert shutil.disk_usage(ROOT).free >= 26 * 1024**3, '25 GiB floor plus 1 GiB estimated build growth required'
  flags = '-fsanitize=address,undefined -fno-omit-frame-pointer'
  environment = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_INCREMENTAL='0', CFLAGS=flags, CXXFLAGS=flags, RUSTFLAGS='-C link-arg=-lasan -C link-arg=-lubsan')
  build = subprocess.run(['cargo', 'test', '-p', 'openpilot-jpeg', '--test', 'ownership', '--no-run', '--locked', '--message-format=json', '-j2'], cwd=ROOT, env=environment, text=True, capture_output=True)
  (args.output / 'build.jsonl').write_text(build.stdout)
  (args.output / 'build.log').write_text(build.stderr)
  assert build.returncode == 0, build.stderr
  binaries = [row['executable'] for line in build.stdout.splitlines() if (row := json.loads(line)).get('reason') == 'compiler-artifact' and row.get('executable')]
  assert len(binaries) == 1
  environment.update(LD_PRELOAD=subprocess.check_output(['g++', '-print-file-name=libasan.so'], text=True).strip(), ASAN_OPTIONS='detect_leaks=1', UBSAN_OPTIONS='halt_on_error=1')
  run = subprocess.run([binaries[0], '--nocapture'], env=environment, text=True, capture_output=True, timeout=60)
  (args.output / 'run.log').write_text(run.stdout + run.stderr)
  assert run.returncode == 0, run.stderr
  configs = list(args.target_dir.glob('debug/build/openpilot-jpeg-*/out/jpeg/CMakeCache.txt'))
  assert len(configs) == 1 and 'CMAKE_C_FLAGS:STRING=' + flags in configs[0].read_text()
  (args.output / 'result.json').write_text(json.dumps({'pass': True, 'binary': binaries[0], 'full_codec_c_flags': flags, 'returncode': run.returncode}, indent=2) + '\n')
  print('PASS: instrumented complete libjpeg-turbo codec,C/CXX owned boundary,ASan+UBSan+leak detection,invalid dimensions and returned bytes ownership')


if __name__ == '__main__':
  main()
