"""Generate the original internal-QCOM/DM models with native verified inputs.

Run with the prepared ARM Python, -I -B -S and -X pycache_prefix=EMPTY_PATH. Generation uses
the retained Python/tinygrad producer and the source-built Rust QCOM compiler.
The real Rust catalog consumer validates each kind before pointer publication.
"""
from __future__ import annotations

import argparse
import ctypes
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
import time

# Isolated Python does not put this script's directory on sys.path.
if __name__ == '__main__' and sys.pycache_prefix is None:
  raise SystemExit('native model generation requires -X pycache_prefix=EMPTY_PRIVATE_PATH before source imports')
sys.path.insert(0, str(Path(__file__).resolve().parent))
import native_model_cache
from native_model_inputs import (InputError, digest, encoded, outside,
                                read_json, regular, sha, source_identity)

SOURCE_ROOTS = ('rust/tools/model_export', 'tinygrad_repo/tinygrad', 'openpilot/common',
                'openpilot/selfdrive/modeld', 'openpilot/system/camerad/cameras')
SOURCE_FILES = ('rust/tools/native_model_inputs.py', 'rust/tools/native_model_cache.py',
                'rust/tools/native_model_generation.py', 'rust/tools/build_offline_model_pipelines.py',
                'pyproject.toml')
MODELS = {'driving': 'openpilot/selfdrive/modeld/models/driving_supercombo.onnx',
          'driver': 'openpilot/selfdrive/modeld/models/dmonitoring_model.onnx'}
ENVIRONMENT = dict(DEV='CPU:LLVM', CPU_COUNT='1', OPENBLAS_NUM_THREADS='1', OMP_NUM_THREADS='1',
                   PYTHON_ZSTANDARD_IMPORT_POLICY='cext', CACHELEVEL='0', LANG='C.UTF-8')


def git(root, *args):
  return subprocess.check_output(['/usr/bin/git', '-C', root, *args])


def source_closure(root):
  """Bind code bytes and additions/removals, including parent import packages.

  Deliberately conservative within the Python producer packages, but unrelated
  Rust and docs commits do not invalidate models. No whole-commit cache key.
  """
  tracked = set(git(root, 'ls-files', '-z').decode().rstrip('\0').split('\0'))
  selected = set(SOURCE_FILES)
  for prefix in SOURCE_ROOTS:
    for path in (root/prefix).rglob('*'):
      relative = path.relative_to(root).as_posix()
      if '__pycache__' in path.parts or path.is_dir(): continue
      if path.suffix != '.py' and prefix != 'tinygrad_repo/tinygrad': continue
      if path.suffix == '.pyc': continue
      if relative not in tracked: raise InputError('untracked producer input or import shadow')
      selected.add(relative)
    for name in tracked:
      if name.startswith(prefix+'/') and (name.endswith('.py') or prefix == 'tinygrad_repo/tinygrad'):
        selected.add(name)
    parent = Path(prefix).parent
    while parent != Path('.'):
      selected.add((parent/'__init__.py').as_posix())
      parent = parent.parent
  result = {}
  for name in sorted(selected):
    path = root/name
    if name not in tracked:
      if path.exists() or path.is_symlink() or not name.endswith('/__init__.py'):
        raise InputError('producer input is missing or is an untracked package shadow')
      result[name] = None
    else:
      result[name] = dict(bytes=path.lstat().st_size, sha256=digest(path, path.lstat().st_size))
  return dict(schema=1, files=result)


def model_inputs(root, commit):
  result = {}
  for kind, name in MODELS.items():
    length = int(git(root, 'cat-file', '-s', f'HEAD:{name}'))
    if not 0 < length <= 128*1024**2: raise InputError('source model exceeds its byte bound')
    data = git(root, 'show', f'HEAD:{name}')
    if len(data) != length: raise InputError('source model changed while reading Git')
    if data.startswith(b'version https://git-lfs.github.com/spec/v1\n'):
      match = re.fullmatch(rb'version https://git-lfs.github.com/spec/v1\noid sha256:([0-9a-f]{64})\nsize ([1-9][0-9]*)\n', data)
      if not match or not 0 < int(match[2]) <= 128*1024**2: raise InputError('invalid source model LFS pointer')
      result[kind] = dict(path=name, sha256=match[1].decode(), bytes=int(match[2]), lfs=True,
          url=f'https://media.githubusercontent.com/media/bin9208/openpilot-rust/{commit}/{name}')
    else:
      if digest(root/name, length) != sha(data): raise InputError('model differs from the current Git blob')
      result[kind] = dict(path=name, sha256=sha(data), bytes=length, lfs=False)
  return result


def command_json(argv, report):
  with report.open('xb') as stream: subprocess.run(list(map(str, argv)), stdout=stream, check=True)
  return read_json(report, 16*1024**2)


def module_origins(root, closure, prefixes):
  for module in list(sys.modules.values()):
    filename = getattr(module, '__file__', None)
    if filename is None: continue
    path = Path(filename).resolve()
    if path.is_relative_to(root):
      name = path.relative_to(root).as_posix()
      if closure['files'].get(name) is None:
        raise InputError('loaded producer module is outside the source input closure')
    elif not any(path.is_relative_to(prefix) for prefix in prefixes):
      raise InputError('loaded module is outside authenticated Python, wheels and source')


def prepare_environment(args, temporary):
  python = command_json([args.preparer, '--prepare-python', '--plan', args.root/'rust/native-python-toolchain.json',
      '--cache', args.python_cache, '--store', args.python_store], temporary/'python.json')
  if (Path(python['python']).resolve() != Path(sys.executable).resolve()
      or Path(python['root']).resolve() != Path(sys.prefix).resolve()):
    raise InputError('model generation must run in the independently prepared native Python')
  with (temporary/'wheels-stdout.json').open('xb') as output:
    subprocess.run(list(map(str, [sys.executable, '-I', '-B', '-S', '-X', 'pycache_prefix='+sys.pycache_prefix,
      args.root/'rust/tools/native_model_inputs.py',
      '--root', args.root, '--preparer', args.preparer, '--cache', args.wheel_cache,
      '--store', args.wheel_store, '--environment-store', args.environment_store,
      '--output', temporary/'wheels.json'])), stdout=output, check=True)
  # native_model_inputs emits its report to --output, not stdout.
  return python, read_json(temporary/'wheels.json', 16*1024**2)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ('root', 'preparer', 'compiler', 'compiler-build', 'consumer', 'bundle-consumer', 'python-cache', 'python-store', 'wheel-cache',
               'wheel-store', 'environment-store', 'model-cache', 'output', 'report'):
    parser.add_argument('--'+name, type=Path, required=True)
  parser.add_argument('--compiler-timeout', type=float, default=60)
  args = parser.parse_args()
  started = time.monotonic()
  if (tuple(sys.version_info[:3]) != (3, 12, 16) or platform.machine() != 'aarch64'
      or not sys.flags.isolated or not sys.flags.no_site or not sys.flags.dont_write_bytecode):
    parser.error('prepared ARM Python 3.12.16 with -I -B -S required')
  bytecode = Path(sys.pycache_prefix)
  if (not bytecode.is_absolute() or bytecode.is_symlink()
      or (bytecode.exists() and (not bytecode.is_dir() or any(bytecode.iterdir())))):
    parser.error('bytecode lookup prefix must be an empty private absolute directory or unused path')
  args.root = args.root.resolve(strict=True)
  origin = source_identity(args.root)
  stores = [outside(args.root, getattr(args, n)) for n in
            ('python_cache', 'python_store', 'wheel_cache', 'wheel_store', 'environment_store', 'model_cache', 'output')]
  for i, path in enumerate(stores):
    for other in stores[i+1:]:
      if path.is_relative_to(other) or other.is_relative_to(path): raise InputError('model generation stores overlap')
  report = args.report.parent.resolve(strict=True)/args.report.name
  if report.is_relative_to(args.root) or any(report.is_relative_to(p) or p.is_relative_to(report) for p in stores):
    raise InputError('model generation report overlaps immutable inputs or source')
  compilers = read_json(args.root/'rust/native-model-compilers.json')
  for library in (compilers['cpu_llvm'], compilers['qcom']):
    if digest(Path(library['path']), library['bytes']) != library['sha256']:
      raise InputError('model compiler library differs from the pinned factory image')
  binaries = {name: dict(sha256=digest(getattr(args, name)), bytes=getattr(args, name).stat().st_size)
              for name in ('compiler', 'consumer', 'bundle_consumer', 'preparer')}
  build = read_json(args.compiler_build)
  build_sha256 = digest(args.compiler_build)
  if (build['schema'] != 1 or build['compiler_sha256'] != binaries['compiler']['sha256']
      or build['compiler_bytes'] != binaries['compiler']['bytes']
      or not re.fullmatch('[0-9a-f]{40}', build['source']['commit'])
      or not re.fullmatch('[0-9a-f]{40}', build['source']['tree'])
      or sorted(build['features']) != ['default', 'native-skip-miri']):
    raise InputError('compiler build record does not match the actual binary and feature selection')
  for name in binaries:
    with regular(getattr(args,name)) as stream: header = stream.read(64)
    if len(header) != 64 or header[:7] != b'\x7fELF\x02\x01\x01' or int.from_bytes(header[18:20],'little') != 183:
      raise InputError('model generation requires genuine native ARM executables')
  with tempfile.TemporaryDirectory(prefix='model-generation-', dir=report.parent) as temp:
    temporary = Path(temp)
    python, wheels = prepare_environment(args, temporary)
    environment_counts = dict(transferred_bytes=0, stored_bytes=0, copied_bytes=0, linked_bytes=0)
    def count_environment(python_row, wheel_row):
      for name in ('transferred_bytes','stored_bytes'):
        environment_counts[name] += python_row['asset'][name]+wheel_row[name]
      environment_counts['copied_bytes'] += python_row['copied_bytes']+wheel_row['copied_bytes']
      environment_counts['linked_bytes'] += wheel_row['linked_bytes']
    count_environment(python,wheels)
    models = model_inputs(args.root, origin['commit'])
    assets = dict(schema=1, reserve_bytes=65536, assets={f'model-{k}':
        {n: p[n] for n in ('url', 'sha256', 'bytes')} for k, p in models.items() if p['lfs']})
    (temporary/'assets.json').write_bytes(encoded(assets))
    receipts = command_json([args.preparer, '--prepare-assets', '--plan', temporary/'assets.json',
                             '--cache', args.model_cache], temporary/'assets-receipts.json')
    paths = {}
    for kind, model in models.items():
      path = Path(receipts['model-'+kind]['path']) if model['lfs'] else args.root/model['path']
      if digest(path, model['bytes']) != model['sha256']: raise InputError('prepared ONNX differs from current source')
      paths[kind] = path
    closure = source_closure(args.root)
    # Eliminate inherited tinygrad knobs before any producer import. The cache
    # identity contains all selected code-generation controls, not job paths.
    os.environ.clear()
    os.environ.update(ENVIRONMENT, PATH='/usr/bin:/bin', LLVM_PATH=compilers['cpu_llvm']['path'],
                      CACHEDB=str(temporary/'tinygrad.sqlite'))
    # Keep authenticated stdlib lookup first. Project roots cannot override an
    # unimported stdlib module, and module origins below reject unbound project
    # shadows/sourceless pyc. -B alone would still READ stale source bytecode.
    script_parent = str(Path(__file__).resolve().parent)
    sys.path[:] = [p for p in sys.path if p != script_parent]+[
        wheels['site_packages'], str(args.root/'rust/tools'), str(args.root/'tinygrad_repo'), str(args.root)]
    from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler
    from model_export.offline_pipeline import OfflinePipelineSpec
    from model_export.offline_publish import emit
    from openpilot.system.camerad.cameras.nv12_info import get_nv12_info
    from tinygrad.runtime.autogen import llvm
    def cpu_input():
      # CPULLVMCompiler resolves its original "native" CPU through these exact
      # APIs. Bind that input without changing original CPU math/codegen policy.
      return dict(architecture=platform.machine(),
          processor=ctypes.string_at(llvm.LLVMGetHostCPUName()).decode(),
          features=ctypes.string_at(llvm.LLVMGetHostCPUFeatures()).decode())
    cpu = cpu_input()
    prefixes = [Path(python['root']).resolve(), Path(wheels['root']).resolve()]
    module_origins(args.root, closure, prefixes)
    compiler = OfflineQcomCompiler(CompilerCommand((str(args.compiler), '--library', compilers['qcom']['path']), args.compiler_timeout))
    common = dict(schema=1, target='aarch64-unknown-linux-gnu', producer=closure,
        python_tree_sha256=python['tree_sha256'], wheel_tree_sha256=wheels['tree_sha256'],
        compiler=binaries['compiler'], compiler_inputs={k:v for k,v in build.items() if k!='source'},
        compiler_arguments=['--library', compilers['qcom']['path']], compiler_timeout=float(args.compiler_timeout),
        factory=compilers, environment=ENVIRONMENT, renderer='a630,IMAGE_PITCH_ALIGNMENT=64',
        schedule=dict(IMAGE=1, FLOAT16=1, NOLOCALS=1, BEAM=0, OPENPILOT_HACKS=1, CAPTURING=1, NO_MEMORY_PLANNER=1),
        python_hash_randomization=bool(sys.flags.hash_randomization))
    common['python_bytecode_lookup'] = 'empty-private-prefix-with-dont-write-bytecode'
    common['cpu_codegen_host'] = cpu

    def revalidate():
      module_origins(args.root, closure, prefixes)
      if bytecode.exists() and any(bytecode.iterdir()): raise InputError('private bytecode prefix changed during generation')
      if cpu_input() != cpu: raise InputError('native CPU compiler inputs changed during generation')
      if source_identity(args.root) != origin or source_closure(args.root) != closure:
        raise InputError('model producer source changed during generation')
      for kind, model in models.items():
        if digest(paths[kind], model['bytes']) != model['sha256']: raise InputError('ONNX changed during generation')
      for name, value in binaries.items():
        if digest(getattr(args, name), value['bytes']) != value['sha256']: raise InputError('native model tool changed during generation')
      if digest(args.compiler_build) != build_sha256: raise InputError('compiler build record changed during generation')
      for library in (compilers['cpu_llvm'], compilers['qcom']):
        if digest(Path(library['path']), library['bytes']) != library['sha256']: raise InputError('model compiler library changed')
      with tempfile.TemporaryDirectory(prefix='recheck-', dir=temporary) as t:
        again_python, again_wheels = prepare_environment(args, Path(t))
      count_environment(again_python,again_wheels)
      if (again_python['tree_sha256'] != python['tree_sha256'] or again_wheels['tree_sha256'] != wheels['tree_sha256']):
        raise InputError('model preparation environment changed during generation')

    bundle_validation = []
    def validate(catalog):
      result = subprocess.run([args.consumer, '--check-catalog', catalog], capture_output=True, check=True)
      value = json.loads(result.stdout)
      if (value['kernels_executed'] or len(value['bundles']) != 2
          or {tuple(b['camera']) for b in value['bundles']} != {(1928,1208),(1344,760)}
          or any(b['backend'] != 'qcom-cl' or b['kind'].lower() != kind for b in value['bundles'])):
        raise InputError('native consumer did not validate the complete two-camera catalog')
      pointer = read_json(catalog/'current.json')
      generation = catalog/pointer['generation']
      for entry in read_json(generation/'index.json')['bundles']:
        checked = subprocess.run([args.bundle_consumer,'--check-bundle',generation/entry['directory']],
                                 capture_output=True,check=True)
        bundle = json.loads(checked.stdout)
        if (bundle['gpu_executed'] or bundle['backend'] != 'qcom-cl' or bundle['arch'] != 'a630'
            or bundle['kernels'] <= 0 or bundle['calls'] <= 0):
          raise InputError('native consumer did not validate QCOM weights, programs and dispatch')
        bundle_validation.append(dict(kind=kind,camera=entry['camera'],**bundle))

    args.output.mkdir(exist_ok=True)
    results = {}
    for kind in ('driving', 'driver'):
      calls_before, kind_started = compiler.compile_calls, time.monotonic()
      requests = []
      for camera in ((1928, 1208), (1344, 760)):
        stride, y_height, uv_height, size = get_nv12_info(*camera)
        contract = dict(schema=1, kind=kind, camera=list(camera), common=common,
            model={key: models[kind][key] for key in ('path', 'sha256', 'bytes', 'lfs')},
            nv12=dict(stride=stride, y_height=y_height, uv_height=uv_height, bytes=size))
        spec = OfflinePipelineSpec(kind, paths[kind], models[kind]['sha256'], camera)
        def produce(destination, spec=spec): emit(spec, compiler, destination)
        requests.append((contract, max(64*1024**2, models[kind]['bytes']*12), produce))
      results[kind] = native_model_cache.publish(args.output/(kind+'-models'), requests, origin, validate, revalidate)
      results[kind].update(qcom_compiler_calls=compiler.compile_calls-calls_before,
                           elapsed_ms=int(1000*(time.monotonic()-kind_started)))
    revalidate()
    result = dict(schema=1, source=origin, kinds=results,
        transferred_bytes=environment_counts['transferred_bytes']+sum(r['transferred_bytes'] for r in receipts.values()),
        stored_bytes=environment_counts['stored_bytes']+sum(r['stored_bytes'] for r in receipts.values()),
        environment_preparation=environment_counts,
        model_assets=receipts, original_models=models, consumer=binaries['consumer'],
        bundle_consumer=binaries['bundle_consumer'], bundle_validation=bundle_validation, compiler_build=build,
        cpu_codegen_host=cpu,
        elapsed_ms=int(1000*(time.monotonic()-started)), full_model_generation=True,
        full_model_regression=False, full_runtime_build=False, gpu_executed=False, physical_device_execution=False)
    with report.open('xb') as output: output.write(encoded(result))


if __name__ == '__main__':
  main()
