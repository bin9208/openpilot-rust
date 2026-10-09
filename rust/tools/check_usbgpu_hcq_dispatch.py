from __future__ import annotations
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import subprocess

from usbgpu_hcq_data import Artifact
from usbgpu_hcq_dispatch import dispatcher, sources


class Boundary:
  def __init__(self, failure=None):
    self.trace, self.reads, self.failure = [], 0, failure

  def call(self, request):
    response = {'result': request['size'] if request['kind'] == 'control' else 0}
    if not self.trace and self.failure is not None:
      response['result'] = -1 if self.failure == 'negative_control' else request['size'] // 2
    if request['kind'] == 'bulk' and request['endpoint'] & 128:
      self.reads += 1
      word = [0, 3, 0, 3][self.reads - 1] if self.reads <= 4 else 3
      response['data'] = list(word.to_bytes(8, 'little')[: request['size']])
    self.trace.append({'request': request, 'response': response})
    return response


def original(parameters, library_path, position, failure=None):
  boundary = Boundary(failure)

  def control(handle, kind, request, value, index, pointer, size, timeout):
    response = boundary.call(
      {
        'kind': 'control',
        'handle': handle,
        'type': kind,
        'request': request,
        'value': value,
        'index': index,
        'size': size,
        'timeout': timeout,
        'data': list(ctypes.string_at(pointer, size)),
      }
    )
    return response['result']

  def bulk(handle, endpoint, pointer, size, transferred, timeout):
    request = {'kind': 'bulk', 'handle': handle, 'endpoint': endpoint, 'size': size, 'timeout': timeout}
    if not endpoint & 128:
      request['data'] = list(ctypes.string_at(pointer, size))
    response = boundary.call(request)
    if endpoint & 128:
      ctypes.memmove(pointer, bytes(response['data']), size)
    if transferred:
      ctypes.c_int.from_address(transferred).value = size
    return response['result']

  callbacks = [
    ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_uint64, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int, ctypes.c_int)(
      control
    ),
    ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_uint64, ctypes.c_int, ctypes.c_void_p, ctypes.c_int, ctypes.c_uint64, ctypes.c_int)(bulk),
  ]
  buffers = []
  for parameter in parameters:
    data, name, slot = bytearray(parameter['bytes']), parameter['name'], parameter['slot']
    if slot in (0, 3):
      data[:8] = ctypes.cast(callbacks[slot // 3], ctypes.c_void_p).value.to_bytes(8, 'little')
    elif name.startswith('usb_host'):
      data[:8] = (0x1234).to_bytes(8, 'little')
    elif name.startswith('put_value'):
      data[:8] = position.to_bytes(8, 'little')
    elif name.startswith(('inputs', 'addr')):
      for i in range(len(data) // 8):
        data[i * 8 : i * 8 + 8] = (0x80000000 + slot * 0x100000 + i * 4096).to_bytes(8, 'little')
    buffers.append((ctypes.c_ubyte * len(data)).from_buffer_copy(data))
  library = ctypes.CDLL(str(library_path.resolve()))
  library.run.argtypes, library.run.restype = [ctypes.POINTER(ctypes.c_void_p)], None
  library.run((ctypes.c_void_p * len(buffers))(*[ctypes.addressof(buffer) for buffer in buffers]))
  outputs = [
    {'slot': p['slot'], 'sha256': hashlib.sha256(bytes(buffer)).hexdigest()} for p, buffer in zip(parameters, buffers, strict=True) if p['slot'] not in (0, 3)
  ]
  return {'trace': boundary.trace, 'buffers': outputs}


def native(binary, evidence, position):
  boundary = Boundary()
  command = [str(binary), str(evidence / 'nodes.json'), str(evidence / 'parameters.json'), str(position)]
  with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
    done = None
    for line in process.stdout:
      request = json.loads(line)
      if 'done' in request:
        done = request['done']
        break
      process.stdin.write(json.dumps(boundary.call(request)) + '\n')
      process.stdin.flush()
    process.stdin.close()
    stderr = process.stderr.read()
    code = process.wait(timeout=10)
  return {'trace': boundary.trace, 'buffers': done['buffers'] if done else None, 'exit_code': code, 'stderr': stderr, 'invocation': command}


def main():
  parser = argparse.ArgumentParser(description='Compare trusted artifact C code against the native HCQ dispatcher using owned USB callbacks.')
  parser.add_argument('--model', type=Path, required=True)
  parser.add_argument('--runtime', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(exist_ok=True, parents=True)
  with Artifact(args.model, args.runtime) as artifact:
    program = next(r for r in artifact.records if r.global_name.endswith('.UOp') and artifact.op(r) == 'PROGRAM')
    nodes, parameters = dispatcher(artifact, program)
    source = sources(program)[2].arguments[2]
    model_hash = artifact.sha256
  (args.evidence / 'nodes.json').write_text(json.dumps(nodes))
  (args.evidence / 'parameters.json').write_text(json.dumps(parameters))
  wrapper = '\nvoid run(void **p) { hcq_submit(' + ','.join(f'p[{i}]' for i in range(len(parameters))) + '); }\n'
  c_file, library = args.evidence / 'source.c', args.evidence / 'source.so'
  c_file.write_text(source + wrapper)
  compile_command = ['cc', '-shared', '-fPIC', '-O2', str(c_file), '-o', str(library)]
  compiled = subprocess.run(compile_command, capture_output=True, text=True, check=True)
  (args.evidence / 'compile.log').write_text(json.dumps({'invocation': compile_command, 'exit_code': compiled.returncode}) + '\n' + compiled.stderr)
  rows = []
  for position in (0, 262142):
    source_result, native_result = original(parameters, library, position), native(args.binary, args.evidence, position)
    for kind, result in [('source', source_result), ('native', native_result)]:
      (args.evidence / f'{kind}-{position}.json').write_text(json.dumps(result, indent=2))
    passed = source_result['trace'] == native_result['trace'] and source_result['buffers'] == native_result['buffers'] and native_result['exit_code'] == 0
    rows.append({'position': position, 'passed': passed, 'source_calls': len(source_result['trace']), 'native_calls': len(native_result['trace'])})
    print(position, 'PASS' if passed else 'FAIL', flush=True)
  failures = []
  for failure in ('negative_control', 'short_control'):
    result = original(parameters, library, 0, failure)
    (args.evidence / f'source-{failure}.json').write_text(json.dumps(result, indent=2))
    failures.append({'failure': failure, 'first_result': result['trace'][0]['response']['result'], 'source_calls_after_failure': len(result['trace']) - 1})
  (args.evidence / 'comparison.json').write_text(
    json.dumps(
      {
        'results': rows,
        'source_failures': failures,
        'model_sha256': model_hash,
        'numeric_contract': 'exact transfer fields, payload bytes and all non-function host buffers; function addresses normalized by binding',
      },
      indent=2,
    )
  )
  assert all(row['passed'] for row in rows)
  assert all(row['source_calls_after_failure'] > 0 for row in failures)


if __name__ == '__main__':
  main()
