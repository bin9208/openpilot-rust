#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import base64
from enum import StrEnum
import json
from pathlib import Path
from typing import assert_never

import anyio
from aiohttp import ClientSession
from carrot_server_support_fixture import Fixture, packet
from carrot_server_terminal_cli_fixture import Inputs
from carrot_server_terminal_pty import output
from carrot_server_support_roles import run as roles, primary, expiry
from carrot_server_support_assets import run as assets
from carrot_server_support_tunnel_control import overlong


class Scenario(StrEnum):
  APPROVE = 'approve'
  ROLES = 'roles'
  ASSETS = 'assets'
  PRIMARY = 'primary'
  EXPIRY = 'expiry'


async def approve(fixture: Fixture) -> None:
  async with ClientSession() as client:
    async with client.ws_connect(fixture.url + '/ws/terminal_pty') as local:
      local_meta = await local.receive_json()
      async with client.post(fixture.url + '/api/support_terminal/start', json={'note': 'owned issue', 'ttl_seconds': 900, 'command_timeout_seconds': 15}) as response:
        body = await response.text()
        await fixture.record('start-http.json', {'status': response.status, 'body': body})
        assert response.status == 200
        session = json.loads(body)
      assert session['state'] == 'sharing' and session['ttl_seconds'] == 900
      assert session['command_timeout_seconds'] == 15 and session['permission_mode'] == 'approve_each'
      pin = fixture.pin()
      guest_origin = session['local_url'].split('/support/terminal/')[0]
      async with client.ws_connect(fixture.url + '/ws/support_terminal/owner') as owner:
        await packet(owner, 'session_status')
        async with client.ws_connect(guest_origin + '/ws/support_terminal/' + session['id']) as guest:
          await guest.send_json({'type': 'auth', 'pin': pin})
          auth = await packet(guest, 'auth_ok')
          guest_meta = await packet(guest, 'meta')
          assert auth['owner_present'] and auth['control_granted']
          assert guest_meta['pid'] == local_meta['pid'] and not guest_meta['primary']
          await guest.send_json({'type': 'input', 'data': "printf 'OWNED_APPROVED\\n'"})
          requested = await packet(owner, 'command_request')
          await packet(guest, 'command_waiting_approval')
          async with client.post(fixture.url + f'/api/support_terminal/commands/{requested["id"]}/approve') as response:
            assert response.status == 200 and (await response.json())['ok']
          transcript = bytearray()
          running = False
          with anyio.fail_after(6):
            while not running or b'OWNED_APPROVED\r\n' not in transcript:
              message = await guest.receive_json()
              if message['type'] == 'pty_output':
                transcript.extend(base64.b64decode(message['b64']))
              elif message['type'] == 'command_running':
                running = True
          await local.send_json({'type': 'input', 'data': "printf 'OWNED_LOCAL\\n'"})
          await output(local, b'OWNED_LOCAL\r\n')
          await guest.send_json({'type': 'input', 'data': "printf 'REJECTED_MUST_NOT_RUN\\n'"})
          rejected = await packet(owner, 'command_request')
          async with client.post(fixture.url + f'/api/support_terminal/commands/{rejected["id"]}/reject') as response:
            assert response.status == 200 and (await response.json())['ok']
          await packet(guest, 'command_rejected')
          await guest.send_json({'type': 'input', 'data': 'expired'})
          expired = await packet(owner, 'command_request')
          with anyio.fail_after(19):
            while True:
              event = await guest.receive_json()
              if event['type'] == 'command_expired':
                assert event['id'] == expired['id']
                break
          async with client.post(fixture.url + f'/api/support_terminal/commands/{expired["id"]}/approve') as response:
            assert response.status == 400 and (await response.json())['error'] == 'command not pending'
          await fixture.record('approve-result.json', {'same_pty': True, 'guest_primary': guest_meta['primary'], 'approved_output': transcript.decode(errors='replace'), 'reject': True, 'command_expiry': True})
      async with client.post(fixture.url + '/api/support_terminal/stop') as response:
        assert response.status == 200 and (await response.json()) == {'ok': True, 'active': False, 'state': 'idle'}
      await local.send_json({'type': 'input', 'data': "printf 'PTY_AFTER_STOP\\n'"})
      await output(local, b'PTY_AFTER_STOP\r\n')
      await fixture.record('stop-result.json', {'pty_retained': True})


async def run(args: argparse.Namespace) -> None:
  inputs = Inputs(args.command, args.launcher, args.binding, args.vision_root)
  fixture = Fixture(args.output.resolve(), inputs, args.server, args.provider == 'source', args.application)
  try:
    await fixture.start()
    if args.overlong:
      await overlong(fixture, args.overlong)
    else:
      match args.scenario:
        case Scenario.ROLES:
          await roles(fixture)
        case Scenario.ASSETS:
          await assets(fixture)
        case Scenario.PRIMARY:
          await primary(fixture)
        case Scenario.EXPIRY:
          await expiry(fixture)
        case Scenario.APPROVE:
          await approve(fixture)
        case unreachable:
          assert_never(unreachable)
  finally:
    await fixture.close()


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['server', 'command', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--provider', choices=['source', 'native'], required=True)
  parser.add_argument('--application', action='store_true')
  parser.add_argument('--scenario', type=Scenario, choices=list(Scenario), default=Scenario.APPROVE)
  parser.add_argument('--overlong', choices=['stdout', 'stderr'])
  args = parser.parse_args()
  await anyio.Path(args.output.resolve()).mkdir(parents=True)
  await run(args)


if __name__ == '__main__':
  anyio.run(main)
