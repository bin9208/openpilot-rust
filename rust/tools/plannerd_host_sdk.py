"""Actions-only native acados SDK for x64 hosts below the wheel's Haswell ISA.

The generator wheel remains pinned. Foreign C libraries use the exact upstream
Git revisions already recorded with its licenses, compiled for the actual host.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
LIBRARIES = ('libacados.so', 'libblasfeo.so', 'libhpipm.so', 'libqpOASES_e.so.3.1', 'libqpOASES_e.so')
REPOSITORIES = {'acados': 'acados/acados', 'blasfeo': 'giaf/blasfeo',
                'hpipm': 'giaf/hpipm', 'qpoases': 'acados/qpOASES'}
# The frozen ARM notice renders nine form feeds as the visible text ^L.
# Pin the actual upstream bytes separately; never weaken either byte check.
QPOASES_LICENSE_SHA256 = '5df07007198989c622f5d41de8d703e7bef3d0e79d62e24332ee739a452af62a'


def digest(path):
  with Path(path).open('rb') as body:
    return hashlib.file_digest(body, 'sha256').hexdigest()


def encoded(value):
  return (json.dumps(value, sort_keys=True, separators=(',', ':'))+'\n').encode()


def verify_license(row, source, notice):
  upstream, bundled = source.read_bytes(), notice.read_bytes()
  raw_sha = hashlib.sha256(upstream).hexdigest()
  notice_sha = hashlib.sha256(bundled).hexdigest()
  expected = QPOASES_LICENSE_SHA256 if row['name'] == 'qpoases' else row['sha256']
  if raw_sha != expected or notice_sha != row['sha256']:
    raise ValueError(f"{row['name']} license mismatch: source expected={expected} actual={raw_sha}; "
                     f"notice expected={row['sha256']} actual={notice_sha}")
  rendered = upstream.replace(b'\x0c', b'^L') if row['name'] == 'qpoases' else upstream
  if rendered != bundled:
    raise ValueError(f"{row['name']} notice differs beyond its documented form-feed representation")
  return dict(source_sha256=raw_sha, bundled_notice_sha256=notice_sha,
              visible_form_feeds=upstream.count(b'\x0c') if row['name'] == 'qpoases' else 0)


def flags():
  rows = [set(line.split(':', 1)[1].split()) for line in Path('/proc/cpuinfo').read_text().splitlines()
          if line.startswith('flags')]
  if not rows:
    raise ValueError('x64 host CPU flags unavailable')
  return set.intersection(*rows)


def tree(root):
  rows = {}
  for path in sorted(root.rglob('*')):
    if path.is_symlink():
      raise ValueError('host SDK must contain materialized regular files')
    if path.is_file() and path.name != 'receipt.json':
      rows[path.relative_to(root).as_posix()] = dict(bytes=path.stat().st_size, sha256=digest(path))
  return rows


def prepare():
  if platform.machine() != 'x86_64':
    raise ValueError('native host SDK requires x86_64')
  features = flags()
  if {'avx2', 'fma'} <= features:
    return None
  if (os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('RUNNER_NAME') != 'bin-actions'
      or os.environ.get('RUNNER_ENVIRONMENT') != 'self-hosted'):
    raise ValueError('pinned wheel requires AVX2/FMA; compatible source SDK is Actions-only')
  job = Path(os.environ['CI_JOB_TEMP'])
  context = {key: os.environ[key] for key in ('GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB')}
  owner = ':'.join(context.values())
  if (job.parent != Path('/tmp') or not job.name.startswith('openpilot-ci.')
      or job.resolve() != job or (job/'owner').read_text() != owner):
    raise ValueError('host SDK requires this job-owned work directory')
  pins = json.loads((ROOT/'rust/crates/plannerd/licenses/sources.json').read_text())
  revisions = {row['name']: row['source'].split('/blob/')[1].split('/')[0] for row in pins['licenses']}
  assert revisions['acados'] == pins['acados_revision']
  notice_pins = {}
  for row in pins['licenses']:
    actual = digest(ROOT/'rust/crates/plannerd/licenses'/(row['name']+'.txt'))
    if actual != row['sha256']:
      raise ValueError(f"{row['name']} frozen notice differs from its original pin")
    notice_pins[row['name']] = actual
  target = 'X64_INTEL_SANDY_BRIDGE' if 'avx' in features else 'GENERIC'
  compilers = {}
  for name in ('gcc', 'g++', 'cmake'):
    path = Path(shutil.which(name) or name).resolve(strict=True)
    compilers[name] = dict(path=str(path), sha256=digest(path))
  recipe = dict(schema=1, revisions=revisions, target=target, compilers=compilers,
                builder_sha256=digest(__file__), upstream_recipe=pins['source_recipe'],
                bundled_notice_sha256=notice_pins)
  identity = hashlib.sha256(encoded(recipe)).hexdigest()
  sdk = job/('planner-host-sdk-'+identity)
  receipt = sdk/'receipt.json'
  if sdk.exists():
    if sdk.is_symlink() or receipt.is_symlink() or stat.S_IMODE(receipt.stat().st_mode) != 0o444:
      raise ValueError('mutable or redirected host SDK receipt')
    record = json.loads(receipt.read_text())
    if record['recipe'] != recipe or record['context'] != context or record['files'] != tree(sdk):
      raise ValueError('host SDK inputs, context or actual output changed')
    return sdk, record
  if shutil.disk_usage(job).free < 2 * 1024**3:
    raise OSError('host SDK requires estimated operation growth of 2 GiB')
  work = job/('planner-host-build-'+identity)
  work.mkdir()
  commands = []
  environment = dict(os.environ, GIT_TERMINAL_PROMPT='0', OPENBLAS_NUM_THREADS='1',
                     GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null')

  def run(argv, label, cwd=None, retry=False):
    for attempt in range(4 if retry else 1):
      started = time.monotonic_ns()
      with (work/(label+f'-{attempt}.log')).open('x') as output:
        child = subprocess.run(list(map(str, argv)), cwd=cwd, env=environment,
                               stdout=output, stderr=subprocess.STDOUT, check=False, timeout=1800)
      commands.append(dict(argv=list(map(str, argv)), returncode=child.returncode,
                           elapsed_ns=time.monotonic_ns()-started))
      if child.returncode == 0:
        return
      if retry and attempt < 3:
        time.sleep((15, 30, 60)[attempt])
      else:
        child.check_returncode()

  def checkout(name, destination):
    destination.mkdir(parents=True, exist_ok=True)
    git = ['git', '-c', 'core.hooksPath=/dev/null', '-C', destination]
    run([*git, 'init', '-q'], name+'-init')
    run([*git, 'fetch', '--depth=1', 'https://github.com/'+REPOSITORIES[name]+'.git', revisions[name]],
        name+'-fetch', retry=True)
    run([*git, 'checkout', '--detach', revisions[name]], name+'-checkout')
    actual = subprocess.check_output([*map(str, git), 'rev-parse', 'HEAD'], text=True).strip()
    if actual != revisions[name]:
      raise ValueError('host C source differs from independently pinned revision')
    return git

  source = work/'source'
  roots = {'acados': source}
  gits = [checkout('acados', source)]
  for name in ('blasfeo', 'hpipm', 'qpoases'):
    path = 'external/'+name
    entry = subprocess.check_output(['git', '-C', str(source), 'ls-tree', 'HEAD', path], text=True)
    if entry.split()[:2] != ['160000', 'commit'] or entry.split()[2] != revisions[name]:
      raise ValueError('licensed dependency differs from pinned acados gitlink')
    roots[name] = source/path
    gits.append(checkout(name, roots[name]))
  licenses = {}
  for row in pins['licenses']:
    name = row['source'].rsplit('/', 1)[1]
    licenses[row['name']] = verify_license(row, roots[row['name']]/name,
        ROOT/'rust/crates/plannerd/licenses'/(row['name']+'.txt'))
  install = work/'install'
  run([compilers['cmake']['path'], '-S', source, '-B', work/'build', '-DCMAKE_BUILD_TYPE=Release',
       '-DCMAKE_C_COMPILER='+compilers['gcc']['path'], '-DCMAKE_CXX_COMPILER='+compilers['g++']['path'],
       '-DBUILD_SHARED_LIBS=ON', '-DACADOS_WITH_QPOASES=ON', '-DBLASFEO_TARGET='+target,
       '-DACADOS_INSTALL_DIR='+str(install), '-DCMAKE_INSTALL_PREFIX='+str(install),
       '-DCMAKE_POLICY_VERSION_MINIMUM=3.5', '-DCMAKE_INSTALL_RPATH=$ORIGIN',
       '-DCMAKE_BUILD_WITH_INSTALL_RPATH=ON', '-DCMAKE_INSTALL_RPATH_USE_LINK_PATH=OFF',
       '-DCMAKE_C_FLAGS=-Wno-implicit-function-declaration -Wno-incompatible-pointer-types',
       '-DCMAKE_CXX_FLAGS='], 'configure')
  run([compilers['cmake']['path'], '--build', work/'build', '--parallel', '2', '--target', 'install'], 'build')
  for git in gits:
    run([*git, 'diff', '--exit-code', 'HEAD'], 'source-clean-'+str(len(commands)))
  sdk.mkdir()
  (sdk/'lib').mkdir()
  for name in LIBRARIES:
    path = (install/'lib'/name).resolve(strict=True)
    if not path.is_relative_to(install.resolve()) or not path.is_file():
      raise ValueError('host library escaped the owned installation')
    shutil.copyfile(path, sdk/'lib'/name)
  for path in (install/'include').rglob('*'):
    if path.is_symlink():
      raise ValueError('unexpected host SDK header alias')
  shutil.copytree(install/'include', sdk/'include', symlinks=False)
  (sdk/'licenses').mkdir()
  for row in pins['licenses']:
    target = sdk/'licenses'/(row['name']+'.txt')
    shutil.copyfile(roots[row['name']]/row['source'].rsplit('/', 1)[1], target)
    if digest(target) != licenses[row['name']]['source_sha256']:
      raise ValueError('upstream license changed while staging the host SDK')
  record = dict(recipe=recipe, context=context, files=tree(sdk), commands=commands,
                licenses=licenses,
                source_built_foreign_libraries=True, vehicle_provider_changed=False,
                host_cpu_flags=sorted(features))
  receipt.write_bytes(encoded(record))
  for path in sdk.rglob('*'):
    if path.is_file():
      path.chmod(0o444)
  for path in sorted((p for p in sdk.rglob('*') if p.is_dir()), reverse=True):
    path.chmod(0o555)
  sdk.chmod(0o555)
  if tree(sdk) != record['files']:
    raise ValueError('host SDK changed during immutable publication')
  return sdk, record
