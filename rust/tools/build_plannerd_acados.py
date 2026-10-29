#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]
WHEELS = {
  'x86_64': '3b451852e83d62815cead999ab31073db9be60307f772650336cdd4534f12b9e',
  'aarch64': '2ad9fcebef1f65112a9cebe8a093977310602e1325d442dc345beaf512679211',
}
LIBRARIES = ('libacados.so', 'libblasfeo.so', 'libhpipm.so', 'libqpOASES_e.so.3.1')


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def disk_guard(path: Path) -> int:
  available = shutil.disk_usage(path).free
  if available < 25 * 1024**3 + 128 * 1024**2:
    raise RuntimeError(f'planner build requires 25 GiB free plus 128 MiB growth; available={available}')
  return available


def run(command: list[str], directory: Path, label: str, environment: dict[str, str]) -> None:
  available = disk_guard(directory)
  begin = time.monotonic_ns()
  with (directory / (label + '.stdout')).open('w') as stdout, (directory / (label + '.stderr')).open('w') as stderr:
    result = subprocess.run(command, cwd=directory, env=environment, stdout=stdout, stderr=stderr, check=False)
  receipt = {
    'command': command,
    'returncode': result.returncode,
    'elapsed_ns': time.monotonic_ns() - begin,
    'free_bytes_before': available,
    'growth_budget_bytes': 128 * 1024**2,
  }
  (directory / (label + '.json')).write_text(json.dumps(receipt, indent=2) + '\n')
  result.check_returncode()


def stage_libraries(wheel: Path, output: Path, architecture: str) -> None:
  if digest(wheel) != WHEELS[architecture]:
    raise ValueError('acados wheel does not match uv.lock and target architecture')
  disk_guard(output)
  with zipfile.ZipFile(wheel) as archive:
    for library in (*LIBRARIES, 'libqpOASES_e.so'):
      suffix = '/acados/install/lib/' + library
      matches = [name for name in archive.namelist() if name.endswith(suffix)]
      if len(matches) != 1:
        raise ValueError(f'wheel library is missing or ambiguous: {library}')
      (output / library).write_bytes(archive.read(matches[0]))
    marker = '/acados/install/include/'
    for member in archive.namelist():
      if marker not in member or member.endswith('/'):
        continue
      relative = Path(member.split(marker, 1)[1])
      if relative.is_absolute() or '..' in relative.parts:
        raise ValueError('invalid wheel header path')
      target = output / 'sdk/include' / relative
      target.parent.mkdir(parents=True, exist_ok=True)
      target.write_bytes(archive.read(member))


def verify_generator(wheel: Path, acados: Path) -> dict[str, str]:
  architecture = platform.machine()
  if architecture not in WHEELS or digest(wheel) != WHEELS[architecture]:
    raise ValueError('generator wheel must match the pinned host architecture wheel')
  hashes = {}
  with zipfile.ZipFile(wheel) as archive:
    for member in archive.namelist():
      marker = '.data/purelib/'
      if marker not in member or member.endswith('/'):
        continue
      relative = Path(member.split(marker, 1)[1])
      if relative.parts[0] not in ('acados', 'casadi'):
        continue
      if relative.is_absolute() or '..' in relative.parts:
        raise ValueError('invalid generator wheel path')
      expected = hashlib.sha256(archive.read(member)).hexdigest()
      path = acados.parent / relative
      if not path.is_file() or digest(path) != expected:
        raise ValueError(f'installed generator differs from pinned wheel: {relative}')
      hashes[str(relative)] = expected
  if not hashes:
    raise ValueError('generator wheel has no verified acados/CasADi files')
  return hashes


def stage_licenses(output: Path) -> dict[str, str]:
  source = ROOT / 'rust/crates/plannerd/licenses'
  destination = output / 'licenses'
  destination.mkdir()
  hashes = {}
  for path in sorted(source.iterdir()):
    if path.is_file():
      shutil.copy2(path, destination / path.name)
      hashes['licenses/' + path.name] = digest(path)
  return hashes


def build(args: argparse.Namespace) -> None:
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  if (output / 'manifest.json').exists():
    raise ValueError('use a new output directory to preserve the existing solver artifact')
  acados = args.acados.resolve()
  python = str(args.python.absolute())
  generator_wheel = args.generator_wheel or args.wheel
  generator_hashes = verify_generator(generator_wheel.resolve(), acados)
  stage_libraries(args.wheel.resolve(), output, args.architecture)
  license_hashes = stage_licenses(output)
  include = output / 'sdk/include'
  environment = dict(
    os.environ,
    PYTHONPATH=f'{ROOT}:{ROOT / "opendbc_repo"}',
    OPENBLAS_NUM_THREADS='1',
    PYTHONDONTWRITEBYTECODE='1',
    ACADOS_SOURCE_DIR=str(acados / 'install'),
    ACADOS_PYTHON_INTERFACE_PATH=str(acados / 'acados_template'),
    TERA_PATH=str(acados / 'install/bin/t_renderer'),
  )
  compiler_path = Path(shutil.which(args.cc) or args.cc).resolve(strict=True)
  compiler_identity = {
    'path': str(compiler_path),
    'sha256': digest(compiler_path),
    'version': subprocess.check_output([str(compiler_path), '--version'], env=environment, text=True),
  }
  source_hashes = {str(path.relative_to(output)): digest(path) for path in include.rglob('*') if path.is_file()}
  for kind, name in (('lateral', 'lat'), ('longitudinal', 'long')):
    source = ROOT / f'openpilot/selfdrive/controls/lib/{kind}_mpc_lib/{name}_mpc.py'
    directory = output / 'generated' / kind
    directory.mkdir(parents=True, exist_ok=True)
    disk_guard(directory)
    shutil.copy2(source, directory / source.name)
    source_hashes[str(source.relative_to(ROOT))] = digest(source)
    run([python, str(ROOT / 'rust/tools/plannerd_generate_source.py'), str(directory / source.name)], directory, 'generate', environment)
    generated = directory / 'c_generated_code'
    sources = [generated / f'acados_solver_{name}.c']
    sources += [generated / f'{name}_model/{name}_{part}.c' for part in ('expl_ode_fun', 'expl_vde_forw')]
    sources += [generated / f'{name}_cost/{name}_cost_y{stage}_{part}.c' for stage in ('', '_e', '_0') for part in ('fun', 'fun_jac_ut_xt', 'hess')]
    if name == 'long':
      sources += [generated / f'long_constraints/long_constr_h_{part}.c' for part in ('fun', 'fun_jac_uxt_zt')]
    compiler = [
      args.cc,
      '-std=gnu11',
      '-O2',
      '-g',
      '-fPIC',
      '-DACADOS_WITH_QPOASES',
      '-I',
      str(include),
      '-I',
      str(include / 'blasfeo/include'),
      '-I',
      str(include / 'hpipm/include'),
      '-I',
      str(generated),
      '-shared',
    ]
    if args.architecture == 'aarch64':
      compiler += ['-mcpu=cortex-a57', '-D__TICI__']
    if args.sanitize:
      compiler += ['-fsanitize=address,undefined', '-fno-omit-frame-pointer', '-fno-sanitize-recover=all']
    linker = ['-Wl,--disable-new-dtags', '-Wl,-rpath,$ORIGIN', '-L', str(output), '-lm', '-lacados', '-lhpipm', '-lblasfeo', '-lqpOASES_e']
    run(compiler + [str(path) for path in sources] + linker + ['-o', str(output / f'libacados_ocp_solver_{name}.so')], directory, 'compile', environment)
    for path in generated.rglob('*'):
      if path.is_file():
        source_hashes[str(path.relative_to(output))] = digest(path)
  files = [*LIBRARIES, 'libqpOASES_e.so', 'libacados_ocp_solver_lat.so', 'libacados_ocp_solver_long.so']
  machine = {'x86_64': 62, 'aarch64': 183}[args.architecture]
  for name in files:
    header = (output / name).read_bytes()[:20]
    if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != machine:
      raise ValueError(f'wrong ELF ABI in {name}')
  manifest = {
    'format': 1,
    'acados': '0.2.2.post103',
    'abi': 'ocp-double-i32',
    'architecture': args.architecture,
    'wheel_sha256': digest(args.wheel),
    'files': [{'name': name, 'sha256': digest(output / name)} for name in files],
  }
  (output / 'provenance.json').write_text(
    json.dumps(
      {
        'sources': source_hashes,
        'generator_wheel_sha256': digest(generator_wheel),
        'generator_files': generator_hashes,
        'compiler': compiler_identity,
        'licenses': license_hashes,
        'sanitizers': bool(args.sanitize),
      },
      indent=2,
    )
    + '\n'
  )
  (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', required=True, type=Path)
  parser.add_argument('--python', required=True, type=Path)
  parser.add_argument('--acados', required=True, type=Path)
  parser.add_argument('--wheel', required=True, type=Path)
  parser.add_argument('--generator-wheel', type=Path, help='pinned host wheel; required when the target differs from the host')
  parser.add_argument('--architecture', required=True, choices=tuple(WHEELS))
  parser.add_argument('--cc', default='gcc')
  parser.add_argument('--sanitize', action='store_true', help='instrument generated C for the isolated native-boundary checks')
  build(parser.parse_args())


if __name__ == '__main__':
  main()
