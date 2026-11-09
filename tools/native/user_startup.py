"""Normal device startup: prepare changed inputs once, compile, then run."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess


def module(name):
  spec = importlib.util.spec_from_file_location('user_startup_' + name, Path(__file__).with_name(name + '.py'))
  value = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(value)
  return value


early = module('early_startup')
# Bootstrap-tool dependencies and the offline model compiler. Other application
# .rs edits are handled by Cargo without preparing SDKs/models again.
HELPER_CRATES = ('runtime-build', 'runtime-installer', 'dev-tools', 'disk-space',
                 'usbgpu', 'params', 'http-transport', 'model-runtime')


def git(source, *args):
  return subprocess.check_output(['/usr/bin/git', '-C', str(source), *args],
      env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0'}, timeout=30)


def inputs_key(source):
  # Git already received/checked these immutable objects. Compare Git object
  # identities rather than rereading SDK/model archives or all extracted files.
  entries = git(source, 'ls-tree', '-r', '-z', 'HEAD', '--',
      'rust', 'tools/native', 'openpilot/cereal', 'opendbc', 'opendbc_repo', 'panda', 'tinygrad_repo',
      'openpilot/selfdrive/modeld/models')
  selected = []
  for row in entries.split(b'\0'):
    if not row: continue
    name = os.fsdecode(row.split(b'\t', 1)[1])
    path = Path(name)
    if (any(name.startswith('rust/crates/' + crate + '/') for crate in HELPER_CRATES)
        or name.startswith(('rust/tools/',
                         'openpilot/cereal/', 'opendbc/', 'opendbc_repo/', 'panda/', 'tinygrad_repo/',
                         'openpilot/selfdrive/modeld/models/'))
        or path.suffix in ('.json', '.lock', '.capnp', '.h', '.hpp', '.c', '.cc', '.cpp', '.S')
        or path.name in ('Cargo.toml', 'build.rs')):
      selected.append(row)
  return hashlib.sha256(b'\0'.join(selected)).hexdigest()


def preparation_key(source, revision='HEAD'):
  # Compiler/provider setup is independent of application/helper .rs edits.
  # Match committed dependency definitions, FFI and model generation inputs.
  entries = git(source, 'ls-tree', '-r', '-z', revision, '--', 'rust', 'openpilot/cereal',
      'opendbc', 'opendbc_repo', 'panda', 'tinygrad_repo', 'openpilot/selfdrive/modeld/models')
  selected = []
  for row in entries.split(b'\0'):
    if not row: continue
    name = os.fsdecode(row.split(b'\t', 1)[1])
    path = Path(name)
    if name in ('rust/native-bootstrap-inputs.json', 'rust/native-machine.json'):
      continue  # Provider selection is compared separately; seed/resource changes are not model changes.
    if (path.name in ('Cargo.toml', 'Cargo.lock', 'build.rs')
        or path.suffix in ('.json', '.capnp', '.h', '.hpp', '.c', '.cc', '.cpp', '.S')
        or name.startswith(('rust/tools/', 'rust/crates/model-runtime/', 'rust/crates/usbgpu/',
                            'openpilot/cereal/', 'opendbc/', 'opendbc_repo/', 'panda/', 'tinygrad_repo/',
                            'openpilot/selfdrive/modeld/models/'))
        or name == 'rust/crates/runtime-build/src/catalog.rs'):
      selected.append(row)
  return hashlib.sha256(b'\0'.join(selected)).hexdigest()


def reusable_preparation(configuration, source, runtime):
  attempts = Path(configuration['stores']['generations'])/'bootstrap-attempts'
  key = preparation_key(source)
  candidates = sorted(attempts.glob('attempt-*/runtime-inputs/result.json'),
                      key=lambda p: p.stat().st_mtime_ns, reverse=True)
  for path in candidates:
    try:
      result = runtime.read(path)
      if result.get('configuration_sha256') != configuration['runtime_inputs']['sha256']:
        continue
      if preparation_key(source, result['source']['commit']) != key:
        continue
      native = runtime.read(Path(result['native_configuration']), result['native_configuration_sha256'])
      if (native['stores'] != configuration['stores']
          or native['cache'] != str(Path(configuration['cache'])/'cargo-build')
          or not (Path(native['toolchain'])/'cargo').is_file()):
        continue
      layout = runtime.read(Path(native['layout']))
      if not Path(native['build_plan']).is_file() or not Path(layout['provider_root']).is_dir():
        continue
      # Reuse paths/recipes, not a claim that any SDK/model/output was audited.
      # Cargo and the running daemons report actual compile/runtime failures.
      return dict(native, resources=configuration['resources'])
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
      continue
  return None


def stamp(path):
  info = Path(path).stat()
  return [info.st_size, info.st_mtime_ns]


def save(path, value):
  temporary = path.with_name(path.name + '.new')
  with temporary.open('w') as out:
    json.dump(value, out)
    out.flush()
    os.fsync(out.fileno())
  os.replace(temporary, path)


def compile_runtime(args, functions, source, machine, pin):
  runtime = functions['runtime']
  cache = Path(machine['cache'])
  cache.mkdir(parents=True, exist_ok=True)
  state_path = cache / 'user-startup.json'
  key = inputs_key(source)
  try:
    state = json.loads(state_path.read_text())
  except FileNotFoundError:
    state = {}
  reusable = (state.get('schema') == 1 and state.get('source') == str(source)
      and state.get('inputs_key') == key and Path(state.get('configuration', '/missing')).is_file()
      and all(Path(p).is_file() and stamp(p) == value for p, value in state.get('helpers', {}).items())
      and bool(state.get('helpers')))
  if not reusable:
    runtime.PROGRESS('Preparing changed build inputs')
    with functions['installed_checkout'](machine, source, args.request_owner) as (clean, observe):
      prepared = functions['build_requested'](args, machine, clean, pin,
          observe_source=observe, prepare_only=True,
          prepared_inputs=lambda config, root: reusable_preparation(config, root, runtime))
    tools = Path(prepared['tools'])
    state = dict(schema=1, source=str(source), inputs_key=key,
        configuration=prepared['configuration'], configuration_sha256=prepared['configuration_sha256'],
        tools=str(tools), helpers={str(tools/name): stamp(tools/name)
          for name in ('openpilot-runtime-native', 'openpilot-native-build-scope')})
    save(state_path, state)
  config = runtime.read(Path(state['configuration']), state['configuration_sha256'])
  head = git(source, 'rev-parse', 'HEAD').decode().strip()
  # Cargo is not rerun for an unchanged clean checkout. Its dependency scanner
  # handles tracked source edits; untracked old packages/captures do not trigger
  # a rebuild. Only small output metadata is inspected here, never file bodies.
  dirty = git(source, 'diff', '--name-only', 'HEAD').decode().splitlines()
  dirty = [p for p in dirty if not (source/p).is_file()
           or state.get('outputs', {}).get(str(source/p)) != stamp(source/p)]
  untracked = git(source, 'ls-files', '--others', '--exclude-standard', '--', 'rust').decode().splitlines()
  dirty += [p for p in untracked if Path(p).suffix in ('.rs', '.toml', '.c', '.cc', '.cpp', '.h', '.hpp')]
  same_outputs = bool(state.get('outputs')) and all(
      Path(p).is_file() and stamp(p) == value for p, value in state.get('outputs', {}).items())
  if state.get('compiled_head') == head and not dirty and same_outputs:
    runtime.PROGRESS('Starting openpilot')
    return state
  owner = functions['seed'].owner_chain(args.request_owner)
  commands = runtime.OwnedCommands(owner, Path(machine['stores']['params']))
  runtime.PROGRESS('Compiling changed source')
  # Available memory can differ on the next boot. Fit the compile cap again;
  # this reads kernel counters, not compiler/SDK/provider trees.
  resources, _ = runtime.memory.fit(machine['resources'])
  if resources != config['resources']:
    config = dict(config, resources=resources)
    path = cache/'user-native-config.json'
    save(path, config)
    state.update(configuration=str(path), configuration_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
    save(state_path, state)
  tools = Path(state['tools'])
  command = ['/usr/bin/sudo', '-n', str(tools/'openpilot-native-build-scope'),
      '--compile-user', '--config', state['configuration'], '--configuration-sha256',
      state['configuration_sha256'], '--root', str(source), '--request-owner', owner]
  log_path = cache/'user-build.log'
  try:
    with runtime.build_cpus(), log_path.open('w+b') as log:
      try:
        output = commands.run(command, env=functions['seed'].ENV, stdout=subprocess.PIPE,
                              stderr=log, check=True, stderr_limit=None)
      except subprocess.CalledProcessError:
        # The memory cap was reached with parallel compilers. Finished units stay
        # in Cargo's target, so continue once with fewer jobs instead of failing.
        if not (cache/'user-build-low-memory').exists():
          raise
        runtime.PROGRESS('Compiling changed source · retrying with fewer jobs')
        output = commands.run(command, env=functions['seed'].ENV, stdout=subprocess.PIPE,
                              stderr=log, check=True, stderr_limit=None)
  except subprocess.CalledProcessError as error:
    raise RuntimeError(f'Compilation failed; log: {log_path}') from error
  result = json.loads(output.stdout)
  if result['development_audit_executed'] or result['sealed_generation_created']:
    raise RuntimeError('normal startup unexpectedly entered a development audit')
  if git(source, 'rev-parse', 'HEAD').decode().strip() != head:
    raise RuntimeError('source changed while compiling; retry the changed source')
  layout = runtime.read(Path(config['layout']))
  # Record dependency/output metadata like an ordinary incremental builder.
  # The large SDK and model/compiler input trees are deliberately absent.
  outputs = {str(p): stamp(p) for p in (source/'rust/bin').iterdir() if p.is_file()}
  for relative in layout['files']:
    path = source/relative
    if path.is_file(): outputs[str(path)] = stamp(path)
  state.update(compiled_head=head, outputs=outputs,
      libraries=result['libraries'], last_build=result)
  save(state_path, state)
  runtime.PROGRESS('Build complete · starting openpilot')
  return state


def display_required():
  return (Path('/AGNOS').is_file() or Path('/TICI').is_file()) and not os.environ.get('CI')


def launch(args, functions):
  source = args.source.resolve(strict=True)
  show = display_required()
  display = early.Startup(source) if show else None
  runtime = functions['runtime']
  try:
    # Recovery/display precede machine/SDK/input preparation and compilation.
    if display:
      display.start()
      runtime.PROGRESS = display.update
    functions['seed'].native()
    if os.geteuid() == 0:
      raise ValueError('device builds run as the normal non-root user')
    machine, pin = functions['installed_machine_configuration'](source, args.machine_config,
        args.machine_configuration_sha256)
    functions['validate_machine'](machine, source)
    if args.running_config is not None or args.previous is not None:
      raise ValueError('normal launch cannot also stage an audited generation')
    state = compile_runtime(args, functions, source, machine, pin)
    env = dict(os.environ)
    for key in list(env):
      if key.startswith('OPENPILOT_NATIVE_') or key in ('OPENPILOT_RUNNING_GENERATION', 'LD_PRELOAD'):
        env.pop(key)
    env['OPENPILOT_USER_SOURCE_BUILD'] = '1'
    env['LD_LIBRARY_PATH'] = os.pathsep.join([str(source/'rust/bin/lib'), *map(str, state.get('libraries', []))])
    command = [str(source/'rust/bin/openpilot-launch'), '--root', str(source)]
    if args.paths is not None: command += ['--paths', str(args.paths)]
    if display: display.handoff()
    os.execve(command[0], command, env)
  except (ValueError, TypeError, KeyError, OSError, RuntimeError, subprocess.SubprocessError) as error:
    if display: display.failure(error)
    raise
