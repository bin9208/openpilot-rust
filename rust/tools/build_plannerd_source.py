import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tomllib

from build_plannerd_acados import LIBRARIES, ROOT, WHEELS, digest, disk_guard, run, verify_generator


def build(args):
  artifact, acados = args.artifact.resolve(), args.acados.resolve()
  manifest = json.loads((artifact / 'manifest.json').read_text())
  architecture = manifest['architecture']
  if architecture not in ('x86_64', 'aarch64'):
    raise ValueError('unsupported source oracle architecture')
  names = {*LIBRARIES, 'libqpOASES_e.so', 'libacados_ocp_solver_lat.so', 'libacados_ocp_solver_long.so'}
  if (
    manifest['format'] != 1
    or manifest['acados'] != '0.2.2.post103'
    or manifest['abi'] != 'ocp-double-i32'
    or manifest['wheel_sha256'] != WHEELS[architecture]
    or len(manifest['files']) != len(names)
    or {file['name'] for file in manifest['files']} != names
  ):
    raise ValueError('pinned source artifact manifest required')
  for library in manifest['files']:
    if digest(artifact / library['name']) != library['sha256']:
      raise ValueError('source artifact differs from its manifest')
  verified = verify_generator(args.generator_wheel.resolve(), acados)
  python = str(args.python.absolute())
  environment = dict(os.environ, PYTHONDONTWRITEBYTECODE='1', OPENBLAS_NUM_THREADS='1')
  query = 'import Cython,json,numpy,sysconfig; print(json.dumps([Cython.__version__,sysconfig.get_path("include"),numpy.get_include()]))'
  version, python_include, numpy_include = json.loads(subprocess.check_output([python, '-c', query], env=environment, text=True))
  locked = tomllib.loads((ROOT / 'uv.lock').read_text())
  expected_version = next(package['version'] for package in locked['package'] if package['name'] == 'cython')
  if version != expected_version:
    raise ValueError('source Cython differs from uv.lock')
  if architecture != platform.machine() and (args.python_include is None or args.numpy_include is None):
    raise ValueError('cross compilation requires target Python and NumPy headers')
  python_include = str(args.python_include.resolve()) if args.python_include else python_include
  numpy_include = str(args.numpy_include.resolve()) if args.numpy_include else numpy_include
  output = args.output.resolve()
  output.parent.mkdir(parents=True, exist_ok=True)
  disk_guard(output.parent)
  output.mkdir(parents=True, exist_ok=False)
  template = acados / 'acados_template'
  include = artifact / 'sdk/include'
  records = []
  for kind, name in (('lateral', 'lat'), ('longitudinal', 'long')):
    generated = artifact / 'generated' / kind / 'c_generated_code'
    destination = output / kind / 'c_generated_code'
    destination.mkdir(parents=True)
    source = destination / 'acados_ocp_solver_pyx.c'
    run(
      [python, '-m', 'cython', '-3', '-o', str(source), '-I', str(generated), '-I', str(template), str(template / 'acados_ocp_solver_pyx.pyx')],
      destination,
      'cython',
      environment,
    )
    command = [
      args.cc,
      '-std=gnu11',
      '-O2',
      '-g',
      '-fPIC',
      '-shared',
      '-DACADOS_WITH_QPOASES',
      '-I',
      python_include,
      '-I',
      numpy_include,
      '-I',
      str(include),
      '-I',
      str(include / 'blasfeo/include'),
      '-I',
      str(include / 'hpipm/include'),
      '-I',
      str(generated),
    ]
    if architecture == 'aarch64':
      command += ['-mcpu=cortex-a57', '-D__TICI__']
    binary = destination / 'acados_ocp_solver_pyx.so'
    command += [
      str(source),
      '-L',
      str(artifact),
      '-Wl,--disable-new-dtags',
      '-Wl,-rpath,' + str(artifact),
      '-lacados_ocp_solver_' + name,
      '-lacados',
      '-lm',
      '-o',
      str(binary),
    ]
    run(command, destination, 'compile', environment)
    header = binary.read_bytes()[:20]
    machine = 62 if architecture == 'x86_64' else 183
    if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != machine:
      raise ValueError('wrong source Cython ELF architecture')
    records.append(
      {
        'kind': kind,
        'source_sha256': digest(source),
        'binary_sha256': digest(binary),
        'template_sha256': digest(template / 'acados_ocp_solver_pyx.pyx'),
        'model_pxd_sha256': digest(generated / 'acados_solver.pxd'),
      }
    )
  (output / 'receipt.json').write_text(
    json.dumps(
      {
        'architecture': architecture,
        'cython': version,
        'artifact_manifest_sha256': digest(artifact / 'manifest.json'),
        'generator_files': verified,
        'bindings': records,
        'python_include': python_include,
        'numpy_include': numpy_include,
      },
      indent=2,
    )
    + '\n'
  )


def main():
  parser = argparse.ArgumentParser()
  for key in ('artifact', 'acados', 'generator-wheel', 'python', 'output'):
    parser.add_argument('--' + key, type=Path, required=True)
  parser.add_argument('--python-include', type=Path)
  parser.add_argument('--numpy-include', type=Path)
  parser.add_argument('--cc', default='gcc')
  build(parser.parse_args())


if __name__ == '__main__':
  main()
