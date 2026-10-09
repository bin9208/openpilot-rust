#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Caller supplies --server --native --launcher --binding --vision-root --output.
# Add --application for the actual Native Application routes/lifecycle.
# ──────────────────
"""Owned terminal HTTP/legacy tmux/CLI-to-real-PTY bridge comparison."""

from __future__ import annotations

import argparse
import base64
import json
from pathlib import Path
import sys

import anyio
from aiohttp import ClientSession
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Inputs
from carrot_server_terminal_http_fixture import Peer
from carrot_server_terminal_pty import output


async def run(output_dir: Path, inputs: Inputs, server: Path, application: bool, start_case: int) -> None:
  peers = [Peer(output_dir / name, name == 'source', inputs, server, application) for name in ['source', 'native']]
  observations = []
  try:
    for peer in peers:
      await peer.start()
    cases = [
      ('GET', '/api/terminal_commands', None),
      ('HEAD', '/api/terminal_commands', None),
      ('GET', '/api/terminal_pty/status', None),
      ('GET', '/api/vision_test/status', None),
      ('HEAD', '/api/vision_test/status', None),
      ('POST', '/api/terminal_commands/run', b'{'),
      ('POST', '/api/terminal_commands/run', b'[]'),
      *[
        ('POST', '/api/terminal_commands/run', json.dumps(body).encode())
        for body in [
          {},
          {'command': 'help', 'args': None},
          {'command': 'help', 'args': ['x'] * 33},
          {'command': 'help', 'args': ['x' * 257]},
          {'command': 'unknown'},
          {'command': '\ud800'},
          {'command': '  HELP  ', 'args': ['vision']},
          {'command': 'web-intro'},
          {'command': 'web-intro', 'args': [12]},
          {'command': 'vision'},
          {'command': 'youtube-test', 'args': ['help']},
        ]
      ],
      ('GET', '/api/terminal_commands/run', None),
      ('POST', '/api/vision_test/status', None),
      ('GET', '/download/tmux.log', None),
    ]
    if application:
      cases = [cases[index] for index in [0, 2, 3, 14, 20]]
      cases.append(('GET', '/api/heartbeat_status', None))
    async with ClientSession() as client:
      for method, path, body in cases[start_case:]:
        pair = []
        for peer in peers:
          async with client.request(method, f'http://127.0.0.1:{peer.port}{path}', data=body, headers={'Content-Type': 'application/json'}) as response:
            data = await response.read()
            text = data.decode().replace(str(peer.root), '$OWNED')
            pair.append({'status': response.status, 'body': text, 'allow': response.headers.get('Allow')})
        observations.append({'method': method, 'path': path, 'source': pair[0], 'native': pair[1], 'equal': pair[0] == pair[1]})
        await anyio.to_thread.run_sync(save, output_dir / 'http-pairs.json', observations)
        assert pair[0] == pair[1], observations[-1]
      wire = []
      for peer in peers:
        async with client.ws_connect(f'http://127.0.0.1:{peer.port}/ws/terminal?session=owned-local') as legacy:
          meta = await legacy.receive_json()
          screen = await legacy.receive_json()
          assert meta['created'] and screen['text'] == 'owned pane\nsecond line'
          await legacy.send_json({'type': 'input', 'data': 'tmux a'})
          reply = await legacy.receive_json()
          assert reply == screen
          await legacy.send_json({'type': 'control', 'action': 'ctrl_c'})
          assert await legacy.receive_json() == screen
        async with client.ws_connect(f'http://127.0.0.1:{peer.port}/ws/terminal_pty') as pty:
          meta = await pty.receive_json()
          await pty.send_json({'type': 'input', 'data': '::web-intro'})
          transcript = await output(pty, b'[[CARROT_WEB_ACTION:web-intro]]')
          assert '변경사항은 저장되지 않습니다.'.encode() in transcript
          wire.append({'pty_meta': meta, 'meta_bridge_output_base64': base64.b64encode(transcript).decode()})
      await anyio.to_thread.run_sync(save, output_dir / 'wire.json', wire)
    await anyio.to_thread.run_sync(
      save,
      output_dir / 'result.json',
      {
        'http_pairs': len(cases) - start_case,
        'all_equal': True,
        'legacy_tmux_adapter_observed': True,
        'actual_pty_to_original_native_cli_bridge': True,
        'application': application,
      },
    )
  finally:
    original_error = sys.exception()
    errors = []
    for peer in peers:
      try:
        with anyio.CancelScope(shield=True):
          await peer.close()
      except (OSError, TimeoutError, anyio.EndOfStream, anyio.BrokenResourceError, ExceptionGroup) as error:
        errors.append(error)
    await anyio.to_thread.run_sync(save, output_dir / 'cleanup.json', {'errors': [str(error) for error in errors]})
    if errors:
      if original_error:
        original_error.add_note('Terminal cleanup errors: ' + repr(errors))
      else:
        raise ExceptionGroup('terminal fixture cleanup', errors)


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['server', 'native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--application', action='store_true')
  parser.add_argument('--start-case', type=int, default=0)
  args = parser.parse_args()
  output_dir = args.output.resolve()
  await anyio.Path(output_dir).mkdir(parents=True)
  await run(
    output_dir,
    Inputs(args.native.resolve(), args.launcher.resolve(), args.binding.resolve(), args.vision_root.resolve()),
    args.server.resolve(),
    args.application,
    args.start_case,
  )


if __name__ == '__main__':
  anyio.run(main)
