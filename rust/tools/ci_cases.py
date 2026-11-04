"""Job-owned independent commands with affinity and aggregate resource admission."""
from concurrent.futures import ThreadPoolExecutor, as_completed
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import threading
import time

MIB = 1024**2


@contextmanager
def readonly_inputs(paths):
  """Temporarily protect only this job's prepared inputs, then restore modes."""
  owner = Path(os.environ['CI_JOB_TEMP']).resolve(strict=True)
  modes = {}
  try:
    for root in paths:
      root = Path(root).resolve(strict=True)
      if not root.is_relative_to(owner):
        raise ValueError('refusing to chmod an input outside this job')
      for path in [root, *root.rglob('*')] if root.is_dir() else [root]:
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode):
          continue
        if not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)) or info.st_uid != os.getuid():
          raise ValueError('prepared input is not an owned file or directory')
        if path not in modes:
          modes[path] = stat.S_IMODE(info.st_mode)
          path.chmod(modes[path] & ~0o222)
    yield
  finally:
    for path, mode in modes.items():
      path.chmod(mode)


def available_memory():
  values = dict(line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines())
  available = int(values['MemAvailable'].split()[0])*1024
  # The host can have more free memory than this job's cgroup permits.
  group = next((line.split(':', 2)[2] for line in Path('/proc/self/cgroup').read_text().splitlines()
                if line.startswith('0::')), None)
  if group is not None:
    root = Path('/sys/fs/cgroup')
    candidates = [root, root/group.lstrip('/')]
    for path in candidates:
      maximum, current = path/'memory.max', path/'memory.current'
      if maximum.is_file() and current.is_file() and maximum.read_text().strip() != 'max':
        available = min(available, max(0, int(maximum.read_text())-int(current.read_text())))
  return available


def admission(path, cases):
  cpus = len(os.sched_getaffinity(0))
  memory, disk = available_memory(), shutil.disk_usage(path).free
  # Preserve each operation's estimate; parallel groups reserve their sum.
  maximum_memory = max(c['memory_bytes'] for c in cases)
  maximum_growth = max(c['growth_bytes'] for c in cases)
  if memory < maximum_memory+128*MIB or disk < maximum_growth+64*MIB:
    raise RuntimeError('insufficient resources for even one case; existing data retained')
  def slots(costs, budget):
    count, used = 0, 0
    for cost in sorted(costs):
      if used+cost > budget:
        break
      used += cost
      count += 1
    return count
  # Mixed groups need their actual sum, not N copies of the largest group.
  # These slot counts only bound the pool. The exact joint disk/memory sum is
  # still reserved under the condition lock immediately before each spawn.
  workers = min(len(cases), cpus,
                slots((c['memory_bytes'] for c in cases), memory-128*MIB),
                slots((c['growth_bytes'] for c in cases), disk-64*MIB))
  if workers < 1:
    raise RuntimeError('insufficient resources for even one case; existing data retained')
  return dict(workers=workers, affinity_cpus=cpus, available_memory_bytes=memory,
              free_disk_bytes=disk, per_worker_memory_bytes=maximum_memory,
              per_worker_growth_bytes=maximum_growth, mode='per-case-aggregate-reservation')


def run_cases(cases, output, cwd):
  output = Path(output)
  output.mkdir(parents=True, exist_ok=True)
  names = [c['name'] for c in cases]
  if not names or len(names) != len(set(names)) or any('/' in n or n in ('.', '..') for n in names):
    raise ValueError('case names must be unique local names')
  policy = admission(output, cases)
  reserved = [0, 0]
  lock, failed = threading.Lock(), threading.Event()
  capacity = threading.Condition(lock)
  rows, active = {}, {}
  peak_processes = 0

  def execute(case):
    nonlocal peak_processes
    name = case['name']
    private = output/name
    private.mkdir()
    temporary = private/'tmp'
    temporary.mkdir()
    environment = dict(os.environ, TMPDIR=str(temporary), TMP=str(temporary), TEMP=str(temporary),
                       OPENPILOT_PREFIX='ci-'+str(os.getpid())+'-'+name)
    environment.update(case.get('env', {}))
    started = time.monotonic()
    row = dict(name=name, argv=list(map(str, case['argv'])), cwd=str(cwd),
               temporary=str(temporary), ipc_prefix=environment['OPENPILOT_PREFIX'],
               started_monotonic=started, state='failed')
    admitted = False
    try:
      with capacity:
        while True:
          if failed.is_set():
            row['state'] = 'not-started'
            return row
          if (shutil.disk_usage(output).free >= reserved[0]+case['growth_bytes']+64*MIB
              and available_memory() >= reserved[1]+case['memory_bytes']+128*MIB):
            break
          if reserved == [0, 0]:
            raise RuntimeError('resources no longer fit even one case')
          # A running case can release capacity. Reduce concurrency before
          # rejecting the operation; never change Cargo/test global limits.
          capacity.wait(timeout=1)
        reserved[0] += case['growth_bytes']
        reserved[1] += case['memory_bytes']
        admitted = True
        row['admitted_monotonic'] = time.monotonic()
      with (private/'stdout').open('wb') as stdout, (private/'stderr').open('wb') as stderr:
        child = subprocess.Popen(row['argv'], cwd=cwd, env=environment, stdout=stdout,
                                 stderr=stderr, start_new_session=True)
        with lock:
          active[name] = child
          row.update(pid=child.pid, spawned_monotonic=time.monotonic())
          peak_processes = max(peak_processes, sum(p.poll() is None for p in active.values()))
          if failed.is_set():
            os.killpg(child.pid, signal.SIGTERM)
        try:
          row['exit_code'] = child.wait()
        finally:
          # Only descendants of this case's freshly owned process group.
          try: os.killpg(child.pid, signal.SIGTERM)
          except ProcessLookupError: pass
          if child.poll() is None:
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired:
              os.killpg(child.pid, signal.SIGKILL)
              child.wait()
          with lock:
            active.pop(name, None)
      if row['exit_code'] != 0:
        raise RuntimeError('case failed; inspect '+str(private/'stderr'))
      row['state'] = 'success'
      return row
    except BaseException as error:
      failed.set()
      row['error'] = str(error)
      raise
    finally:
      if admitted:
        with capacity:
          reserved[0] -= case['growth_bytes']
          reserved[1] -= case['memory_bytes']
          capacity.notify_all()
      row['elapsed_seconds'] = time.monotonic()-started
      row['finished_monotonic'] = time.monotonic()
      (private/'command.json').write_text(json.dumps(row, indent=2)+'\n')
      rows[name] = row

  error = None
  pool = ThreadPoolExecutor(max_workers=policy['workers'])
  try:
    for future in as_completed([pool.submit(execute, case) for case in cases]):
      try: future.result()
      except Exception as failure:
        error = failure
  except BaseException:
    failed.set()
    with lock:
      for child in active.values():
        try: os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError: pass
    raise
  finally:
    pool.shutdown(wait=True)
    complete = set(rows) == set(names) and all(r['state'] == 'success' for r in rows.values())
    (output/'summary.json').write_text(json.dumps(dict(schema=1, admission=policy,
        expected_cases=names, cases=rows, observed_maximum_parallel_processes=peak_processes,
        success=complete), indent=2)+'\n')
  if error: raise error
  if not complete: raise RuntimeError('not every original case completed successfully')
  return rows
