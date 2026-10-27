#!/usr/bin/env python3
"""Build/seal the current native runtime from a fixed seed and content inputs.

This standard-library driver never runs a compiler as root, downloads an
exact-commit runtime release, activates a generation or starts the manager.
The source-built private workers own preparation/build resource enforcement.
"""
import sys

if __name__ == '__main__' and not (sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode):
  raise SystemExit('native source runtime requires /usr/bin/python3 -I -B -S')

import argparse
import ctypes
from contextlib import contextmanager, ExitStack, suppress
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import signal
import stat
import subprocess
import tempfile
import time

_BYTECODE = None
if __name__ == '__main__':
  _BYTECODE = tempfile.TemporaryDirectory(prefix='openpilot-native-driver-bytecode-', dir='/tmp')
  sys.pycache_prefix = _BYTECODE.name
# Admit this exact source file without adding project roots to stdlib lookup or
# accepting a same-named extension module. The private prefix above prevents
# timestamp-based reuse of an ignored project pyc when executing the driver.
_SPEC = importlib.util.spec_from_file_location('openpilot_native_bootstrap_seed_source', Path(__file__).with_name('bootstrap_seed.py'))
seed = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(seed)

TOOLS_FIELDS = {'schema', 'stores', 'cache', 'system_sdk_receipt', 'system_sdk_sha256',
                'expected_source_growth_bytes', 'expected_compile_growth_bytes',
                'remaining_object_growth_bytes', 'resources'}


def owned_child(owner, parent, command):
  # Set parent-death protection in a new single-threaded process, avoiding a
  # preexec_fn in the caller. curl retains this across exec. sudo may clear it
  # on credential changes; native frontends independently watch both PID/start
  # identities and their private workers clean up their own descendants.
  if os.geteuid() == 0 or parent <= 1 or not command or not Path(command[0]).is_absolute():
    raise ValueError('owned command child requires a non-root caller and absolute executable')
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  if libc.prctl(1, signal.SIGTERM, 0, 0, 0):
    raise OSError(ctypes.get_errno(), 'cannot bind native command to its parent')
  if os.getppid() != parent:
    raise ValueError('native command parent died before initialization')
  seed.verify_owners(owner)
  if _BYTECODE is not None:
    _BYTECODE.cleanup()
  os.execve(command[0], command, seed.ENV)


class OwnedCommands:
  """Bound stdout capture and cancel only this command's newly created group."""
  def __init__(self, owner, params=None):
    self.owner = owner
    self.params = params

  def observe(self):
    seed.verify_owners(self.owner)
    if self.params is not None:
      offroad(self.params)

  def run(self, argv, *, env, stdout, stderr, check):
    self.observe()
    with ExitStack() as stack:
      out = stack.enter_context(tempfile.TemporaryFile()) if stdout == subprocess.PIPE else stdout
      err = stack.enter_context(tempfile.TemporaryFile()) if stderr == subprocess.PIPE else stderr
      wrapped = ['/usr/bin/python3', '-I', '-B', '-S', str(Path(__file__).resolve()), '--owned-child', self.owner,
          str(os.getpid()), '--', *map(str, argv)]
      child = subprocess.Popen(wrapped, env=env, stdout=out, stderr=err, start_new_session=True)
      try:
        while child.poll() is None:
          self.observe()
          if any(os.fstat(s.fileno()).st_size > 4*1024**2 for s in (out, err)):
            raise ValueError('native source command output exceeded 4 MiB; worker evidence retained')
          time.sleep(0.1)
        self.observe()
        captured = []
        for original, stream in ((stdout, out), (stderr, err)):
          if os.fstat(stream.fileno()).st_size > 4*1024**2:
            raise ValueError('native source command output exceeded 4 MiB')
          if original == subprocess.PIPE:
            stream.seek(0)
            captured.append(stream.read(4*1024**2+1))
          else:
            captured.append(None)
        result = subprocess.CompletedProcess(argv, child.returncode, *captured)
        if check:
          result.check_returncode()
        return result
      except BaseException:
        # The child starts a new session; never signal the updater, manager or
        # another user's group. Native workers also watch the complete owner
        # chain, independently of this Python intermediary's survival.
        if child.poll() is None:
          with suppress(OSError):
            os.killpg(child.pid, signal.SIGTERM)
          with suppress(subprocess.TimeoutExpired):
            child.wait(timeout=2)
          if child.poll() is None:
            with suppress(OSError):
              os.killpg(child.pid, signal.SIGKILL)
            with suppress(subprocess.TimeoutExpired):
              child.wait(timeout=2)
        raise


def pairs(rows):
  result = {}
  for key, value in rows:
    if key in result:
      raise ValueError('duplicate native build JSON key')
    result[key] = value
  return result


def read(path, expected=None):
  with seed.regular(path) as stream:
    data = stream.read(4*1024**2+1)
  if len(data) > 4*1024**2 or (expected is not None and hashlib.sha256(data).hexdigest() != expected):
    raise ValueError('native build configuration or report differs from its pin or byte bound')
  return json.loads(data, object_pairs_hook=pairs)


def offroad(params):
  with seed.regular(params/'d/IsOnroad') as stream:
    if stream.read(2) != b'0':
      raise ValueError('native source preparation requires verified offroad state')


def absolute(value):
  path = Path(value)
  if not path.is_absolute() or str(path) != value or '..' in path.parts:
    raise ValueError('native build path requires a contained absolute spelling')
  return path


def source_identity(source):
  def git(*args):
    return subprocess.check_output(['/usr/bin/git', '-C', str(source), *args], env=seed.ENV, text=True).strip()
  if Path(git('rev-parse', '--show-toplevel')) != source or git('status', '--porcelain', '--untracked-files=all', '--ignore-submodules=none'):
    raise ValueError('native source driver requires the exact clean repository root')
  return dict(commit=git('rev-parse', 'HEAD'), tree=git('rev-parse', 'HEAD^{tree}'))


def validate(configuration, source):
  if (set(configuration) != TOOLS_FIELDS | {'runtime_inputs'} or configuration['schema'] != 1
      or set(configuration['stores']) != {'sources', 'objects', 'generations', 'params'}
      or set(configuration['runtime_inputs']) != {'path', 'sha256'}):
    raise ValueError('source build needs a complete pinned runtime input preparation configuration')
  paths = [absolute(configuration['cache']), *(absolute(p) for p in configuration['stores'].values())]
  for i, path in enumerate(paths):
    if path.is_symlink() or path.parent.resolve(strict=True) != path.parent or (path.exists() and (path.resolve(strict=True) != path or not path.is_dir())):
      raise ValueError('native source driver store or its parent is redirected')
    if path.is_relative_to(source) or source.is_relative_to(path):
      raise ValueError('native source driver stores must be outside source')
    if any(path.is_relative_to(p) or p.is_relative_to(path) for p in paths[i+1:]):
      raise ValueError('native source driver stores overlap')
  selected = configuration['runtime_inputs']
  read(absolute(selected['path']), selected['sha256'])


@contextmanager
def operation(generations):
  if seed.directory(generations) != generations:
    raise ValueError('native source driver generation store is redirected')
  fd = os.open(generations/'source-driver.lock', os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
  try:
    if not stat.S_ISREG(os.fstat(fd).st_mode):
      raise ValueError('native source driver lock must be regular')
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    parent = seed.directory(generations/'source-requests')
    attempt = Path(tempfile.mkdtemp(prefix='request-', dir=parent))
    yield attempt
  finally:
    os.close(fd)


def tools_from_report(report, source, configuration):
  if (report.get('schema') != 1 or report.get('requested_source') != source
      or report['build']['source']['source'] != source
      or report.get('runtime_activated') is not False or report.get('runtime_started') is not False
      or report.get('runtime_release_downloaded') is not False):
    raise ValueError('current tool report differs from the requested source or preparation protocol')
  tools = absolute(report['tools'])
  if tools.resolve(strict=True) != tools or tools.parent != Path(configuration['cache'])/'tools':
    raise ValueError('current tools escaped their verified content bundle store')
  outputs = report['build']['binaries']
  if not {'openpilot-runtime-inputs', 'openpilot-runtime-native', 'openpilot-native-build-scope'} <= set(outputs):
    raise ValueError('current tool bundle lacks the required fixed protocols')
  if {p.name for p in tools.iterdir()} != set(outputs) | {'manifest.json'}:
    raise ValueError('current tool bundle inventory differs from its source build report')
  pins = {}
  for name, object_pin in outputs.items():
    if not re.fullmatch('[a-z0-9][a-z0-9-]*', name) or object_pin['mode'] != 0o555:
      raise ValueError('unsafe tool name or mutable executable object')
    path = tools/name
    with seed.regular(path) as stream:
      info = os.fstat(stream.fileno())
      actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if info.st_size != object_pin['bytes'] or stat.S_IMODE(info.st_mode) != 0o555 or actual != object_pin['sha256']:
      raise ValueError('current tool bytes differ from their verified source build')
    pins[path] = actual
  manifest = tools/'manifest.json'
  with seed.regular(manifest) as stream:
    if stat.S_IMODE(os.fstat(stream.fileno()).st_mode) != 0o444:
      raise ValueError('current tool bundle manifest is mutable')
    pins[manifest] = hashlib.file_digest(stream, 'sha256').hexdigest()
  return tools, pins


def preserve_configuration(generations, configuration, pin):
  parent = generations/'configurations'
  if seed.directory(parent) != parent:
    raise ValueError('native launch configuration store is redirected')
  with seed.regular(configuration) as stream:
    data = stream.read(4*1024**2+1)
  if len(data) > 4*1024**2 or hashlib.sha256(data).hexdigest() != pin:
    raise ValueError('verified native configuration changed before retention')
  destination = parent/(pin+'.json')
  if not destination.exists():
    with tempfile.TemporaryDirectory(prefix='.pending-configuration-', dir=parent) as temporary:
      path = Path(temporary)/'config.json'
      seed.write(path, data)
      try:
        seed.publish(path, destination)
      except FileExistsError:
        pass
  read(destination, pin)
  with seed.regular(destination) as stream:
    if stat.S_IMODE(os.fstat(stream.fileno()).st_mode) != 0o444:
      raise ValueError('retained native launch configuration is mutable')
  return destination


def build_runtime(prepared, plan, config_path, config_pin, source, request_owner=None,
                  running_config=None, running_pin=None, previous=None, expected_source=None):
  started = time.monotonic()
  seed.verify_native(Path(prepared['root']), plan)
  if os.geteuid() == 0:
    raise ValueError('native source driver must run as the original non-root user')
  if plan.get('protocols', {}).get('request_owner') != 2:
    raise ValueError('full native source build needs a verified seed with request-owner chain protocol 2')
  source = source.resolve(strict=True)
  before = source_identity(source)
  if expected_source is not None and before != expected_source:
    raise ValueError('native requested source changed during seed preparation')
  config_path = config_path.resolve(strict=True)
  configuration = read(config_path, config_pin)
  validate(configuration, source)
  params = Path(configuration['stores']['params'])
  offroad(params)
  staging = previous is not None
  if staging != (running_config is not None) or staging != (running_pin is not None):
    raise ValueError('update staging needs the running configuration, its SHA and previous generation together')
  if staging:
    running_config = running_config.resolve(strict=True)
    if not re.fullmatch('[0-9a-f]{64}', previous) or read(running_config, running_pin)['stores'] != configuration['stores']:
      raise ValueError('running generation/configuration differs from the update stores')
  owner = seed.owner_chain(request_owner)
  commands = OwnedCommands(owner, params)

  def check(pins=None):
    seed.verify_owners(owner)
    offroad(params)
    read(config_path, config_pin)
    selection = configuration['runtime_inputs']
    read(Path(selection['path']), selection['sha256'])
    if staging:
      read(running_config, running_pin)
    if source_identity(source) != before or any(seed.digest(p) != sha for p, sha in (pins or {}).items()):
      raise ValueError('native source, configuration or current tool changed during the request')

  with operation(Path(configuration['stores']['generations'])) as attempt:
    def save(name, value):
      path = attempt/name
      seed.write(path, seed.json_bytes(value))
      return path

    def execute(name, command, pins):
      check(pins)
      path = attempt/(name+'.json')
      # Capture into retained files, then read with a byte bound. Compiler and
      # worker logs remain in their independently monitored native attempts.
      with path.open('xb') as stdout, (attempt/(name+'.stderr')).open('xb') as stderr:
        try:
          commands.run(list(map(str, command)), env=seed.ENV, stdout=stdout, stderr=stderr, check=True)
        except subprocess.SubprocessError as error:
          raise ValueError(f'native source step {name} failed; retained evidence {attempt}') from error
        stdout.flush()
        os.fsync(stdout.fileno())
      check(pins)
      os.chmod(path, 0o444)
      return read(path)

    check()
    small = {k: configuration[k] for k in TOOLS_FIELDS}
    first_config = save('tools-config.json', small)
    first = seed.current_tools(prepared, plan, first_config, source, request_owner=owner, run=commands.run)
    save('seed-current-tools.json', first)
    check()
    tools, tool_pins = tools_from_report(first['current_tools'], before, configuration)
    input_report = execute('prepared-inputs', ['/usr/bin/sudo', '-n', tools/'openpilot-runtime-inputs',
        '--bootstrap-tools-scope', '--config', config_path, '--configuration-sha256', config_pin,
        '--root', source, '--request-owner', owner], tool_pins)
    if (input_report.get('configuration_sha256') != config_pin
        or input_report.get('seed_helper_sha256') != tool_pins[tools/'openpilot-runtime-inputs']):
      raise ValueError('input preparation did not use the verified current helper and pinned request')
    tools, tool_pins = tools_from_report(input_report, before, configuration)
    inputs = input_report['runtime_inputs']
    if (inputs['source'] != before or inputs['configuration_sha256'] != configuration['runtime_inputs']['sha256']
        or inputs['bootstrap_configuration_sha256'] != config_pin or not inputs['runtime_inputs_verified']
        or inputs['runtime_roles'] != 18 or inputs['compiler_input_roles'] != 9):
      raise ValueError('input preparation did not verify the complete requested runtime')
    native_config = absolute(inputs['native_configuration'])
    native_pin = inputs['native_configuration_sha256']
    output = Path(input_report['resource_evidence'])/'runtime-inputs'
    attempts = Path(configuration['stores']['generations'])/'bootstrap-attempts'
    if (output.parent.parent != attempts or output.resolve(strict=True) != output
        or not output.parent.name.startswith('attempt-')
        or native_config != output/'native-config.json' or not inputs['files']):
      raise ValueError('prepared native configuration escaped its input attempt')
    for name, pin in inputs['files'].items():
      path = absolute(name)
      if not path.is_relative_to(output) or path.resolve(strict=True) != path or seed.digest(path) != pin:
        raise ValueError('prepared input output differs from its manifest')
    native = read(native_config, native_pin)
    if (native['stores'] != configuration['stores'] or native['resources'] != configuration['resources']
        or native['cache'] != str(Path(configuration['cache'])/'cargo-build')
        or native['toolchain'] != str(Path(input_report['preparations']['rust']['root'])/'bin')):
      raise ValueError('full build configuration changed its stores or resource policy')
    built = execute('built-generation', ['/usr/bin/sudo', '-n', tools/'openpilot-native-build-scope',
        '--config', native_config, '--configuration-sha256', native_pin, '--build', '--root', source,
        '--request-owner', owner], tool_pins)
    if (not re.fullmatch('[0-9a-f]{64}', built['generation_id']) or built['requested_source'] != before
        or built['configuration_sha256'] != native_pin or built['activated'] or built['staged_for_boot']):
      raise ValueError('full build result differs from the requested source or verified configuration')
    retained_config = preserve_configuration(Path(configuration['stores']['generations']), native_config, native_pin)
    staged = None
    if staging:
      staged = execute('staged-generation', [tools/'openpilot-runtime-native',
          '--config', running_config, '--configuration-sha256', running_pin,
          '--stage-candidate', built['generation_id'], '--candidate-config', retained_config,
          '--candidate-configuration-sha256', native_pin, '--previous', previous,
          '--root', source, '--request-owner', owner], tool_pins)
      pending = staged['pending']
      if (pending['candidate'] != built['generation_id'] or pending['expected_previous'] != previous
          or pending['source_commit'] != before['commit'] or pending['configuration_sha256'] != running_pin
          or staged['activated'] or not staged['staged_for_boot']):
        raise ValueError('staged generation differs from the preserved running launch context')
    check(tool_pins)
    read(retained_config, native_pin)
    result = dict(schema=1, source=before, generation_id=built['generation_id'],
        build_configuration=str(retained_config), prepared_configuration=str(native_config), build_configuration_sha256=native_pin,
        launch_configuration=str(running_config if staging else retained_config),
        launch_configuration_sha256=running_pin if staging else native_pin,
        bootstrap_configuration_sha256=config_pin, seed_source=plan['source'], seed_preparation=prepared, request_owner=owner,
        evidence=str(attempt), full_runtime_build=True, staged_for_boot=staging,
        elapsed_ms=int((time.monotonic()-started)*1000), timing_scope='prepared seed through current tools, input preparation, full build and seal',
        driver_process_kernel_peak_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*1024,
        runtime_activated=False, runtime_started=False, runtime_release_downloaded=False)
    save('result.json', result)
    return result


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ('manifest', 'seed-cache', 'config', 'source'):
    parser.add_argument('--'+name, type=Path, required=True)
  parser.add_argument('--manifest-sha256', required=True)
  parser.add_argument('--configuration-sha256', required=True)
  parser.add_argument('--offline-assets', type=Path)
  parser.add_argument('--request-owner')
  parser.add_argument('--running-config', type=Path)
  parser.add_argument('--running-configuration-sha256')
  parser.add_argument('--previous')
  args = parser.parse_args()
  seed.native()
  if os.geteuid() == 0:
    raise ValueError('native source driver requires the original non-root user')
  configuration = read(args.config, args.configuration_sha256)
  validate(configuration, args.source.resolve(strict=True))
  params = Path(configuration['stores']['params'])
  offroad(params)
  requested = source_identity(args.source.resolve(strict=True))
  seed_cache = args.seed_cache.parent.resolve(strict=True)/args.seed_cache.name
  if seed_cache.is_relative_to(args.source.resolve(strict=True)) or args.source.resolve(strict=True).is_relative_to(seed_cache):
    raise ValueError('native seed cache must be outside the requested source')
  owner = seed.owner_chain(args.request_owner)
  commands = OwnedCommands(owner, params)
  prepared, plan = seed.prepare(args.manifest, args.manifest_sha256, args.seed_cache, args.offline_assets,
      download=lambda asset, path: seed.fetch(asset, path, run=commands.run))
  result = build_runtime(prepared, plan, args.config, args.configuration_sha256, args.source,
      owner, args.running_config, args.running_configuration_sha256, args.previous, expected_source=requested)
  print(json.dumps(result, indent=2))


if __name__ == '__main__':
  try:
    if len(sys.argv) > 1 and sys.argv[1] == '--owned-child':
      if len(sys.argv) < 6 or sys.argv[4] != '--':
        raise ValueError('invalid owned native command arguments')
      owned_child(sys.argv[2], int(sys.argv[3]), sys.argv[5:])
    else:
      main()
  except (ValueError, TypeError, KeyError, OSError, subprocess.SubprocessError) as error:
    print('native source runtime: '+str(error), file=sys.stderr)
    sys.exit(1)
