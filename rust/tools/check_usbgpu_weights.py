from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

from import_usbgpu_hcq import Exporter
from usbgpu_hcq_data import Artifact
from usbgpu_model_fixture import Boundary
from usbgpu_model_oracle import virtual_bytes


def compare(args, boundary, snapshot):
  rows = []
  with Artifact(args.model, args.model.parent / 'egpu-runtime') as artifact:
    exporter = Exporter(artifact)
    exporter.export()
    addresses = {row['index']: row['device'] for row in snapshot['buffers']}
    for item in artifact.records:
      if item.global_name != 'tinygrad.device.Buffer':
        continue
      index = exporter.ids[id(item)]
      initial = item.arguments[5]
      source = memoryview(initial).cast('B')
      actual = virtual_bytes(boundary.gpu, addresses[index], len(source))
      source_sha, native_sha = hashlib.sha256(source).hexdigest(), hashlib.sha256(actual).hexdigest()
      rows.append({'index': index, 'bytes': len(source), 'source_sha256': source_sha, 'native_sha256': native_sha,
                   'address': addresses[index], 'exact': source_sha == native_sha})
      if source_sha != native_sha:
        offset = next(i for i, pair in enumerate(zip(source, actual, strict=True)) if pair[0] != pair[1])
        (args.evidence / f'buffer-{index}-expected-full.bin').write_bytes(source)
        (args.evidence / f'buffer-{index}-native-full.bin').write_bytes(actual)
        rows[-1]['first_difference'] = offset
        rows[-1]['physical_segments'] = [{'virtual': virtual, 'physical': physical, 'bytes': size, 'system': system}
          for virtual, (physical, size, system) in boundary.gpu.mmu.tlb.items()
          if virtual < addresses[index] + len(source) and virtual + size > addresses[index]]
      source.release()
  return rows


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--descriptor', type=Path, required=True)
  parser.add_argument('--model', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  command = [str(args.binary), str(args.firmware), str(args.descriptor), str(args.model), str(args.evidence / 'unused-output.bin')]
  boundary, rows = Boundary(), None
  with (args.evidence / 'stderr.log').open('w') as stderr:
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True,
                          env={**os.environ, 'USBGPU_INSPECT_KERNELS': '1', 'USBGPU_FIXTURE_VRAM_BYTES': str(4 << 30)}) as child:
      try:
        for line in child.stdout:
          request = json.loads(line)
          if request['op'] == 'inspect_bindings':
            rows = compare(args, boundary, request['snapshot'])
            child.terminate()
            break
          child.stdin.write(json.dumps({'value': boundary.call(request)}) + '\n')
          child.stdin.flush()
        code = child.wait(timeout=5)
      finally:
        if child.poll() is None:
          child.kill()
          child.wait()
  result = {'invocation': command, 'scenario': 'Every native GPU initial buffer compared with immutable artifact before model dispatch',
            'exit_code': code, 'kernels': boundary.kernels, 'rows': rows, 'passed': rows is not None and all(row['exact'] for row in rows),
            'copy_implementation': os.environ.get('USBGPU_FIXTURE_SDMA_COPY', 'page-aware'),
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'inference_acceptance': False}
  (args.evidence / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({key: value for key, value in result.items() if key != 'rows'}))
  assert result['passed'] and boundary.kernels == 0 and code == -15


if __name__ == '__main__':
  main()
