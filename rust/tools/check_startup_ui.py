#!/usr/bin/env python3
"""Run the six source/native startup scenes and existing host lifecycle/ASAN checks.

Requires an already running, CI-owned Xvfb DISPLAY. --target is the Cargo profile
output directory containing binaries, examples/, deps/, and build/ (not a triple).
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import traceback

from PIL import Image, ImageChops
from Xlib import X, display
from Xlib.ext import xtest


@dataclass(frozen=True)
class Context:
  root: Path
  target: Path
  output: Path
  raylib: Path
  environment: dict[str, str]
  cxx: str
  rust_toolchain: str

  def run(self, name: str, command: list[str], extra_env: dict[str, str] | None = None) -> None:
    with (self.output / f'{name}.log').open('w') as log:
      log.write('Invocation: ' + json.dumps(command) + '\n')
      log.flush()
      subprocess.run(command, cwd=self.root, env=self.environment | (extra_env or {}), stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)


def require(condition: bool, message: str) -> None:
  if not condition:
    raise RuntimeError(message)


def write_json(path: Path, value: object) -> None:
  path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def render_comparisons(ctx: Context) -> None:
  base = {
    'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
    'kind': 'spinner',
    'ip': '192.0.2.10:6999',
    'updates': ['Building native startup runtime', '42'],
  }
  scenes = {
    'small-spinner': base,
    'large-spinner': dict(
      base,
      config={'big': True, 'large_viewport': True, 'pc': False, 'scale': 1.0},
      updates=[
        'A very long status message testing the native startup progress status ellipsis display and its correct placement above the progress bar ' * 2,
        '88',
      ],
    ),
    'wrapped-spinner': dict(base, updates=['Installing runtime components\n  validating source-compatible text wrapping']),
    'scaled-spinner': dict(base, config={'big': False, 'large_viewport': False, 'pc': True, 'scale': 0.75}),
    'small-text': dict(base, kind='text', text='Startup error\n' + 'A long recovery diagnostic text with word-wrapping and scrolling.\n' * 20, wheel=2.0),
    'large-ko-text': dict(
      base,
      kind='text',
      config={'big': True, 'large_viewport': True, 'pc': False, 'scale': 1.0},
      language='ko',
      text='시작 오류: 런타임 구성 요소를 확인하십시오.\n' + '네트워크 연결 및 로그 업로드 상태 확인\n' * 16,
    ),
  }
  report = []
  states = []
  for name, scene in scenes.items():
    scene_path = ctx.output / f'{name}.json'
    write_json(scene_path, scene)
    native_path = ctx.output / f'native-{name}.png'
    source_path = ctx.output / f'source-{name}.png'
    ctx.run(f'native-{name}', [str(ctx.target / 'examples/startup_ui_render'), str(ctx.root), str(scene_path), str(native_path)])
    ctx.run(f'source-{name}', [sys.executable, str(ctx.root / 'rust/tools/startup_ui_source_render.py'), str(scene_path), str(source_path)])
    native = json.loads(native_path.with_suffix('.json').read_text())
    source = json.loads(source_path.with_suffix('.json').read_text())
    if 'spinner' in name:
      require(native == source, f'{name}: source/native state mismatch')
    else:
      require(
        native['lines'] == source['lines'] and native['scroll']['offset'] == source['offset'] and native['scroll']['velocity'] == source['velocity'],
        f'{name}: source/native text/scroll mismatch',
      )
    states.append(f'PASS {name}: native/source state equal')
    with Image.open(native_path) as a, Image.open(source_path) as b:
      require(a.size == b.size, f'{name}: source/native image dimensions differ')
      diff = ImageChops.difference(a.convert('RGB'), b.convert('RGB'))
      channels = diff.split()
      maximum = ImageChops.lighter(ImageChops.lighter(channels[0], channels[1]), channels[2])
      histogram = maximum.histogram()
      changed = a.width * a.height - histogram[0]
      max_delta = max(index for index, count in enumerate(histogram) if count)
      diff.save(ctx.output / f'diff-{name}.png')
      report.append({'scene': name, 'changed_pixels': changed, 'max_delta': max_delta, 'width': a.width, 'height': a.height})
      # The source and native raylib builds have small rotated-texture edge differences.
      # Record those without disguising them as equality; text layouts are exact gates.
      if 'text' in name:
        require(changed == 0, f'{name}: text rendering differs at {changed} pixels')
  write_json(ctx.output / 'image-comparison.json', report)
  (ctx.output / 'state-comparison.log').write_text('\n'.join(states) + '\n')


def live_interaction(ctx: Context) -> None:
  env = ctx.environment | {'SCALE': '1', 'BIG': '0', 'OFFSCREEN': '0', 'PARAMS_ROOT': str(ctx.output / 'params')}
  connection = display.Display(env['DISPLAY'])
  protocol_errors: list[str] = []
  connection.set_error_handler(lambda error, _request: protocol_errors.append(str(error)))
  try:
    with (ctx.output / 'live-text.log').open('w') as log:
      process = subprocess.Popen(
        [str(ctx.target / 'openpilot-text-window'), '--source-root', str(ctx.root), '--text', 'Actual native window click test'],
        env=env,
        cwd=ctx.root,
        stdout=log,
        stderr=subprocess.STDOUT,
      )
      try:
        window = None
        deadline = time.monotonic() + 30
        while window is None and time.monotonic() < deadline:
          require(process.poll() is None, 'TextWindow exited before its window appeared')
          for candidate in connection.screen().root.query_tree().children:
            if candidate.get_wm_name() == 'Text Viewer' and candidate.get_attributes().map_state == X.IsViewable:
              window = candidate
              break
          if window is None:
            time.sleep(0.05)
        require(window is not None, 'TextWindow did not create a window')
        # Fonts finish loading after window creation. Retry the same real click until
        # the first rendered frame can consume it, bounded by the startup deadline.
        attempts = 0
        while process.poll() is None and time.monotonic() < deadline:
          attempts += 1
          window.set_input_focus(X.RevertToParent, X.CurrentTime)
          coords = connection.screen().root.translate_coords(window, 410, 180)
          xtest.fake_input(connection, X.MotionNotify, x=coords.x, y=coords.y)
          connection.sync()
          time.sleep(0.1)
          xtest.fake_input(connection, X.ButtonPress, 1)
          connection.sync()
          time.sleep(0.12)
          xtest.fake_input(connection, X.ButtonRelease, 1)
          connection.sync()
          try:
            process.wait(timeout=0.5)
          except subprocess.TimeoutExpired:
            continue
        require(process.poll() == 0, 'TextWindow did not exit zero after a real button click')
      finally:
        if process.poll() is None:
          process.kill()
        process.wait()
    with (ctx.output / 'live-spinner.log').open('w') as log:
      process = subprocess.Popen(
        [str(ctx.target / 'openpilot-spinner'), '--source-root', str(ctx.root)],
        env=env,
        cwd=ctx.root,
        stdin=subprocess.PIPE,
        stdout=log,
        stderr=subprocess.STDOUT,
      )
      try:
        require(process.stdin is not None, 'spinner stdin pipe missing')
        process.stdin.write(b'Actual stdin update\n')
        process.stdin.flush()
        time.sleep(0.4)
        process.stdin.write(b'73\n')
        process.stdin.flush()
        time.sleep(0.4)
        require(process.poll() is None, 'spinner exited before signal')
        process.send_signal(signal.SIGINT)
        require(process.wait(timeout=5) == 0, 'spinner did not exit zero on SIGINT')
      finally:
        if process.poll() is None:
          process.kill()
        process.wait()
        if process.stdin is not None:
          process.stdin.close()
    write_json(ctx.output / 'live-result.json', {'text_actual_XTest_click_exit': 0, 'text_click_attempts': attempts, 'spinner_stdin_then_SIGINT_exit': 0})
    (ctx.output / 'live-result.log').write_text('PASS actual native text XTest click exit 0; spinner stdin/SIGINT exit 0\n')
  finally:
    connection.close()
  require(not protocol_errors, f'X protocol errors: {protocol_errors}')


def adapter_asan(ctx: Context) -> None:
  headers = list(ctx.target.glob('build/openpilot-startup-ui-*/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h'))
  libraries = list(ctx.target.glob('deps/libcxx-*.rlib'))
  require(bool(headers) and bool(libraries), '--target must contain Cargo CXX build artifacts and deps')
  header = max(headers, key=lambda path: path.stat().st_mtime).parents[2]
  cxx = max(libraries, key=lambda path: path.stat().st_mtime)
  helpers = ctx.root / 'rust/tools/startup_ui_qa'
  runtime = ctx.output / 'libcxx_runtime.a'
  ctx.run(
    'asan-runtime-build',
    [
      'rustup',
      'run',
      ctx.rust_toolchain,
      'rustc',
      '--edition=2024',
      '--crate-type=staticlib',
      str(helpers / 'cxx_runtime.rs'),
      '--extern',
      f'cxx={cxx}',
      '-L',
      f'dependency={ctx.target / "deps"}',
      '-o',
      str(runtime),
    ],
  )
  executable = ctx.output / 'bridge-asan'
  ctx.run(
    'asan-build',
    [
      ctx.cxx,
      '-std=c++17',
      '-g',
      '-fsanitize=address,undefined',
      '-fno-omit-frame-pointer',
      f'-I{header}',
      f'-I{ctx.root / "rust/crates/startup-ui/native"}',
      f'-I{ctx.raylib / "include"}',
      str(helpers / 'bridge_asan.cc'),
      str(ctx.root / 'rust/crates/startup-ui/native/bridge.cc'),
      str(ctx.root / 'rust/crates/startup-ui/native/raylib_loader.cc'),
      str(ctx.root / 'rust/crates/startup-ui/native/graphics.cc'),
      str(ctx.root / 'rust/crates/startup-ui/native/egl.cc'),
      str(ctx.root / 'rust/crates/startup-ui/native/camera.cc'),
      str(runtime),
      '-ldl',
      '-lpthread',
      '-o',
      str(executable),
    ],
  )
  ctx.run(
    'asan',
    [str(executable), str(ctx.root / 'openpilot/selfdrive/assets/img_spinner_track.png'), str(ctx.output / 'asan.png')],
    {'ASAN_OPTIONS': 'detect_leaks=0:halt_on_error=1', 'UBSAN_OPTIONS': 'halt_on_error=1'},
  )


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
  parser.add_argument('--target', type=Path, required=True, help='Cargo profile output directory, e.g. rust/target/release')
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--display', default=os.environ.get('DISPLAY'), help='existing CI-owned Xvfb display')
  parser.add_argument('--raylib-root', type=Path, default=os.environ.get('STARTUP_UI_RAYLIB_ROOT'))
  parser.add_argument('--raylib-library', type=Path, default=os.environ.get('STARTUP_UI_RAYLIB_LIBRARY'))
  parser.add_argument('--cxx', default='g++')
  parser.add_argument('--rust-toolchain', default='1.94.0')
  args = parser.parse_args()
  require(bool(args.display), 'provide DISPLAY or --display for an existing CI-owned X server')
  require(args.raylib_root is not None and args.raylib_library is not None, 'provide native raylib root and plugin path')
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  environment = dict(
    os.environ,
    DISPLAY=args.display,
    OFFSCREEN='1',
    PYTHONPATH=str(args.root.resolve()),
    PARAMS_ROOT=str(output / 'params'),
    STARTUP_UI_RAYLIB_LIBRARY=str(args.raylib_library.resolve()),
  )
  ctx = Context(args.root.resolve(), args.target.resolve(), output, args.raylib_root.resolve(), environment, args.cxx, args.rust_toolchain)
  write_json(output / 'result.json', {'status': 'running'})
  try:
    render_comparisons(ctx)
    live_interaction(ctx)
    ctx.run('children', [str(ctx.target / 'examples/startup_ui_children')], {'SCALE': '1', 'BIG': '0', 'OFFSCREEN': '0'})
    adapter_asan(ctx)
  except Exception:
    write_json(output / 'result.json', {'status': 'failed', 'traceback': traceback.format_exc()})
    raise
  write_json(
    output / 'result.json',
    {
      'status': 'passed',
      'scenes': 6,
      'text_pixel_equality': True,
      'source_state_equality': True,
      'live_input': True,
      'native_children': True,
      'adapter_asan': True,
      'asan_limit': 'prebuilt external raylib uninstrumented; driver leak checks disabled',
    },
  )
  print(f'PASS startup UI source/render/input/children/ASAN checks: {output / "result.json"}')


if __name__ == '__main__':
  main()
