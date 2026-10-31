"""Run the source-bound builder with explicit, recorded CI host dependencies.

The original file is an input of the immutable ARM MPC provider. Keep its bytes
intact: capacity admission and host-only library compatibility belong here.
"""
import hashlib
import json
import os
import runpy
from pathlib import Path
import platform
import shutil
import sys
import types

from plannerd_host_sdk import LIBRARIES, prepare

ROOT = Path(__file__).resolve().parents[2]


def growth_guard(path):
  free = shutil.disk_usage(path).free
  if free < 128 * 1024**2:
    raise OSError(f'planner operation requires estimated growth of 128 MiB; available={free}')
  return free


def main():
  if len(sys.argv) < 3 or sys.argv[1] != '--':
    raise ValueError('CI adapter requires -- and the unchanged original builder path')
  requested = Path(sys.argv[2]).resolve(strict=True)
  expected = ROOT/'rust/tools/build_plannerd_acados.py'
  source_builder = ROOT/'rust/tools/build_plannerd_source.py'
  if requested not in (expected, source_builder):
    raise ValueError('CI adapter may invoke only the solver and original-binding builders')
  runpy.run_path(str(ROOT/'tools/ci/runner_policy.py'))['require_environment']()
  # An ignored timestamp pyc is never evidence of the frozen builder bytes.
  original = types.ModuleType('build_plannerd_acados')
  original.__file__ = str(expected)
  original_source = expected.read_bytes()
  exec(compile(original_source, str(expected), 'exec'), original.__dict__)
  original.disk_guard = growth_guard
  if requested == source_builder:
    # Its imported run() must use this same explicit CI admission too.
    sys.modules['build_plannerd_acados'] = original
    bindings = types.ModuleType('source_planner_binding_builder')
    bindings.__file__ = str(source_builder)
    exec(compile(source_builder.read_bytes(), str(source_builder), 'exec'), bindings.__dict__)
    sys.argv = [str(requested), *sys.argv[3:]]
    bindings.main()
    return
  base_build = original.build
  base_stage = original.stage_libraries

  def build(args):
    used = None

    def stage(wheel, output, architecture):
      nonlocal used
      # Retain the original wheel/generator/architecture verification first.
      base_stage(wheel, output, architecture)
      if architecture == platform.machine() == 'x86_64':
        used = prepare()
        if used is not None:
          sdk, record = used
          for name in LIBRARIES:
            shutil.copyfile(sdk/'lib'/name, output/name)
          # These are the actual headers used for this host library build.
          # output is a fresh, caller-owned artifact checked by original.build.
          shutil.rmtree(output/'sdk/include')
          shutil.copytree(sdk/'include', output/'sdk/include')
          shutil.copytree(sdk/'licenses', output/'ci-host-licenses')

    original.stage_libraries = stage
    try:
      base_build(args)
    finally:
      original.stage_libraries = base_stage
    path = args.output.resolve()/'provenance.json'
    provenance = json.loads(path.read_text())
    provenance['ci_adapter'] = dict(sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        original_builder_sha256=hashlib.sha256(original_source).hexdigest(),
        operation_growth_bytes=128*1024**2, source_bound_builder_changed=False)
    if used is not None:
      sdk, record = used
      provenance['ci_host_foreign_libraries'] = dict(sdk=str(sdk), receipt_sha256=original.digest(sdk/'receipt.json'),
          recipe=record['recipe'], files=record['files'], context=record['context'],
          licenses=record['licenses'],
          original_generator_wheel_sha256=original.digest(args.generator_wheel or args.wheel),
          vehicle_provider_changed=False)
    path.write_text(json.dumps(provenance, indent=2)+'\n')

  original.build = build
  sys.argv = [str(requested), *sys.argv[3:]]
  original.main()


if __name__ == '__main__':
  main()
