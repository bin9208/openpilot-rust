# /// script
# requires-python = ">=3.12"
# dependencies = ["pygame==2.6.1", "pillow"]
# ///
# Run with the retained source-oracle interpreter and cluster source PYTHONPATH.
from __future__ import annotations

import ast
import hashlib
import itertools
import json
from pathlib import Path
import queue
import sys
from typing import Final

import pygame
from PIL import Image
import cluster_replay_tools as original

SOURCE: Final = Path(original.__file__)


def make_cases(output: Path) -> list:
  output.mkdir(parents=True, exist_ok=True)
  cases = []
  for width, height in [(1100, 520), (800, 400), (1400, 720), (360, 260)]:
    for variant in range(5):
      state = {
        'playback_s': [0.0, 125.99, 1000.0, -5.0, 0.0001][variant],
        'duration_s': [0.0, 300.0, 3600.0, 60.0, 0.0][variant],
        'paused': variant % 2 == 1,
        'status_text': ['Waiting for replay', 'Paused: /owned/log/rlog.zst', '한국어 경로 and long status ' * 12, '', 'Loading'][variant],
        'current_log_folder': ['', '/owned/route/segment-0', 'αβγ log path ' * 20, '--', '/owned/last'][variant],
        'options': {name: (index + variant) % 2 == 0 for index, (name, _) in enumerate(original.OPTION_LABELS)},
        'video_status': ['ROUTE VIDEO WAITING', 'CAMERA: unavailable', 'video', '', None][variant],
        'cutin_status': ['waiting', 'CUT-IN: YES target 11', 'CUT-IN: NO', None, 'CUT-IN: YES ' * 10][variant],
        'data_lines': [f'{index:02}: radarState lead {index * 3.75:.2f} m long value ' + 'x' * 60 for index in range(15)] if variant else [],
        'video_width': 0,
        'video_height': 0,
      }
      video_file = None
      if variant in [1, 2, 4]:
        size = [(0, 0), (32, 18), (1024, 576), (0, 0), (1, 1)][variant]
        state['video_width'], state['video_height'] = size
        pixels = bytes(
          (channel * 71 + x * 17 + y * 13) % 256 if channel < 3 else (x * 23 + y * 19 + 80) % 256
          for y in range(size[1])
          for x in range(size[0])
          for channel in range(4)
        )
        video_file = output / f'video-{variant}.rgba'
        video_file.write_bytes(pixels)
      cases.append(
        {'name': f'{width}x{height}-{variant}', 'width': width, 'height': height, 'state': state, 'video_file': str(video_file) if video_file else None}
      )
  return cases


def render_cases(cases: list, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=True)
  pygame.init()
  fonts = [pygame.font.SysFont('consolas', 16), pygame.font.SysFont('consolas', 14), pygame.font.SysFont('segoeui', 19, bold=True)]
  for case in cases:
    screen = pygame.display.set_mode((case['width'], case['height']), pygame.RESIZABLE)
    state = dict(case['state'])
    state['video_rgba'] = Path(case['video_file']).read_bytes() if case['video_file'] else None
    width, height = screen.get_size()
    seek = pygame.Rect(24, height - 42, max(120, width - 48), 16)
    original._draw_replay_tools(
      pygame,
      screen,
      state,
      original._video_surface(pygame, state),
      *fonts,
      pygame.Rect(24, 72, 116, 36),
      pygame.Rect(150, 72, 92, 36),
      pygame.Rect(252, 72, 92, 36),
      pygame.Rect(354, 72, 92, 36),
      original._option_rects(pygame, width, height),
      seek,
    )
    pygame.display.flip()
    pygame.image.save(screen, output / f'{case["name"]}.png')
    with Image.open(output / f'{case["name"]}.png') as captured:
      (output / f'{case["name"]}.rgba').write_bytes(captured.convert('RGBA').tobytes())
  pygame.quit()


def policy_cases(output: Path) -> None:
  source = ast.parse(SOURCE.read_text())
  function = next(node for node in source.body if isinstance(node, ast.FunctionDef) and node.name == '_run_replay_tools_window')
  loop = next(node for node in function.body if isinstance(node, ast.While))
  event_loop = next(node for node in loop.body if isinstance(node, ast.For))
  prefix = ast.parse('''def source_events(events, width, height, state):
    running = True
    dragging_seek = False
    last_folder = ''
    command_queue = queue.Queue(maxsize=16)
    open_rect = pygame.Rect(24,72,116,36)
    previous_rect = pygame.Rect(150,72,92,36)
    pause_rect = pygame.Rect(252,72,92,36)
    next_rect = pygame.Rect(354,72,92,36)
    seek_rect = pygame.Rect(24,height-42,max(120,width-48),16)
    option_rects = _option_rects(pygame,width,height)
''').body[0]
  prefix.body.extend([ast.For(target=event_loop.target, iter=ast.Name(id='events', ctx=ast.Load()), body=event_loop.body, orelse=[])])
  prefix.body.extend(
    ast.parse('''if not running:
    _put_command(command_queue, {'type': 'closed'})
return list(command_queue.queue), state, dragging_seek
''').body
  )
  namespace = dict(vars(original))
  namespace.update(queue=queue, pygame=pygame, _choose_log_file=lambda _: None)
  exec(compile(ast.fix_missing_locations(ast.Module(body=[prefix], type_ignores=[])), str(SOURCE), 'exec'), namespace)
  cases, expected = [], []
  for width, height, playback, paused in itertools.product([360, 800, 1100, 1400], [260, 400, 520, 720], [0.0, 150.0, 300.0], [False, True]):
    seek = pygame.Rect(24, height - 42, max(120, width - 48), 16)
    boxes = original._option_rects(pygame, width, height)
    events = [
      {'kind': 'key', 'key': pygame.K_LEFT},
      {'kind': 'key', 'key': pygame.K_RIGHT},
      {'kind': 'key', 'key': pygame.K_SPACE},
      {'kind': 'down', 'x': 160, 'y': 80, 'button': 1},
      {'kind': 'down', 'x': 360, 'y': 80, 'button': 1},
      {'kind': 'down', 'x': 260, 'y': 80, 'button': 1},
    ]
    for rect in boxes.values():
      events.extend([{'kind': 'down', 'x': rect.x + 2, 'y': rect.y + 2, 'button': 1}] * 2)
    events.extend(
      [
        {'kind': 'down', 'x': seek.x - 10, 'y': seek.y + 2, 'button': 1},
        {'kind': 'down', 'x': seek.x, 'y': seek.y + 2, 'button': 1},
        {'kind': 'move', 'x': seek.x + seek.width // 2, 'y': 0},
        {'kind': 'up', 'x': seek.right + 20, 'y': 0, 'button': 1},
      ]
    )
    state = {'playback_s': playback, 'duration_s': 300.0, 'paused': paused, 'options': {}}
    typed = []
    for event in events:
      match event['kind']:
        case 'key':
          typed.append(pygame.event.Event(pygame.KEYDOWN, key=event['key']))
        case 'down':
          typed.append(pygame.event.Event(pygame.MOUSEBUTTONDOWN, pos=(event['x'], event['y']), button=event['button']))
        case 'move':
          typed.append(pygame.event.Event(pygame.MOUSEMOTION, pos=(event['x'], event['y'])))
        case 'up':
          typed.append(pygame.event.Event(pygame.MOUSEBUTTONUP, pos=(event['x'], event['y']), button=event['button']))
        case unreachable:
          raise RuntimeError(f'unknown fixture event {unreachable}')
    commands, final_state, dragging = namespace['source_events'](typed, width, height, dict(state))
    cases.append({'width': width, 'height': height, 'state': state, 'events': events})
    expected.append({'commands': commands, 'options': final_state['options'], 'dragging': dragging})
  (output / 'events.json').write_text(json.dumps(cases))
  (output / 'events-original.json').write_text(json.dumps(expected))


def main() -> None:
  output = Path(sys.argv[1]).resolve()
  cases = make_cases(output)
  (output / 'cases.json').write_text(json.dumps(cases))
  render_cases(cases, output / 'original')
  policy_cases(output)
  (output / 'source.json').write_text(
    json.dumps(
      {
        'source': str(SOURCE),
        'sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        'pygame': pygame.version.ver,
        'smoothscale_backend': pygame.transform.get_smoothscale_backend(),
        'frames': len(cases),
        'event_cases': 96,
      },
      indent=2,
    )
  )


if __name__ == '__main__':
  main()
