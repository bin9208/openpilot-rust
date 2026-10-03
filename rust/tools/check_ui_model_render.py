"""Run unchanged source/native renderers in real GL and compare complete captures."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import numpy as np
from PIL import Image
from ui_model_qa.scenes import suite
from ui_model_qa.compare import differences
from ui_model_qa.parameters import reads


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--smoke', action='store_true')
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  root = Path.cwd()
  prefix = f'rust-probe-model-{os.getpid()}'
  namespace = Path('/dev/shm') / f'msgq_{prefix}'
  namespace.mkdir()
  environment = dict(os.environ, OPENPILOT_PREFIX=prefix, PYTHONPATH=f'{root}:{root / "rust/tools"}')
  results = []
  try:
    for big in [True, False]:
      name = 'big' if big else 'small'
      output = args.evidence / name
      output.mkdir(exist_ok=True)
      data = {'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0}, 'language': 'en-US', 'cases': suite(big, args.smoke)}
      input_path = output / 'input.json'
      input_path.write_text(json.dumps(data))
      commands = [
        [sys.executable, str(root / 'rust/tools/ui_model_qa/source.py'), str(input_path), str(output / 'source')],
        [str(args.binary.resolve()), str(root), str(input_path), str(output / 'native')],
      ]
      for label, command in zip(['source', 'native'], commands, strict=True):
        (output / f'{label}-invocation.json').write_text(
          json.dumps({'command': command, 'environment': {k: environment.get(k) for k in ['DISPLAY', 'LD_LIBRARY_PATH', 'OPENPILOT_PREFIX', 'PYTHONPATH']}})
        )
        if label == 'native':
          command = ['strace', '-s', '4096', '-e', 'trace=openat,open,write', '-o', str(output / 'parameters.strace'), *command]
        with (output / f'{label}.log').open('w') as log:
          subprocess.run(command, check=True, env=environment, stdout=log, stderr=subprocess.STDOUT)
      source, native = [json.loads((output / label / 'trace.json').read_text()) for label in ['source', 'native']]
      parameters = reads(output / 'parameters.strace')
      assert set(parameters) == {row['name'] for row in native}, parameters.keys()
      for row in native:
        row['parameter_reads'] = parameters[row['name']]
      (output / 'native-parameter-reads.json').write_text(json.dumps(parameters, indent=2))
      (output / 'native-trace-with-parameters.json').write_text(json.dumps(native))
      failures = list(differences(source, native))
      pixels = []
      for path in sorted((output / 'source').glob('*.png')):
        a, b = [np.asarray(Image.open(output / label / path.name)) for label in ['source', 'native']]
        pixels.append({'name': path.name, 'shape': list(a.shape), 'different_components': int(np.count_nonzero(a != b))})
      result = {
        'display': name,
        'cases': len(data['cases']),
        'frames': sum(len(c['steps']) for c in data['cases']),
        'differences': failures,
        'pixels': pixels,
        'passed': not failures and bool(pixels) and all(p['different_components'] == 0 for p in pixels),
      }
      (output / 'results.json').write_text(json.dumps(result, indent=2))
      results.append(result)
      print(name, result['passed'], len(failures), sum(p['different_components'] for p in pixels), flush=True)
    (args.evidence / 'results.json').write_text(json.dumps({'passed': all(r['passed'] for r in results), 'displays': results}, indent=2))
    return 0 if all(r['passed'] for r in results) else 1
  finally:
    for path in namespace.iterdir():
      path.unlink()
    namespace.rmdir()


if __name__ == '__main__':
  sys.exit(main())
