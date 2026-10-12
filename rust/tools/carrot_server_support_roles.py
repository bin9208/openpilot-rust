# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

from aiohttp import ClientSession, WSMsgType
import anyio
import time
from carrot_server_support_fixture import Fixture, packet
from carrot_server_terminal_pty import output


async def run(fixture: Fixture) -> None:
  async with ClientSession() as client:
    async with client.post(fixture.url + '/api/support_terminal/start', json={'permission_mode': 'allow_all', 'ttl_seconds': 3600, 'command_timeout_seconds': 120}) as response:
      session = await response.json()
      assert response.status == 200 and session['permission_mode'] == 'allow_all'
      assert session['ttl_seconds'] == 3600 and session['command_timeout_seconds'] == 120
    pin = fixture.pin()
    origin = session['local_url'].split('/support/terminal/')[0]
    endpoint = origin + '/ws/support_terminal/' + session['id']
    async with client.ws_connect(endpoint) as first:
      await first.send_json({'type': 'auth', 'pin': pin})
      auth = await packet(first, 'auth_ok')
      assert auth['control_granted'] and not auth['owner_present']
      await packet(first, 'meta')
      await packet(first, 'control_role')
      await first.send_json({'type': 'input', 'data': 'owner_absent'})
      await packet(first, 'owner_absent')
      async with client.ws_connect(endpoint) as second:
        await second.send_json({'type': 'auth', 'pin': pin})
        auth = await packet(second, 'auth_ok')
        assert not auth['control_granted']
        await packet(second, 'meta')
        role = await packet(second, 'control_role')
        assert not role['granted']
        async with client.ws_connect(fixture.url + '/ws/support_terminal/owner') as owner:
          await packet(owner, 'session_status')
          await packet(first, 'owner_presence')
          await second.send_json({'type': 'input', 'data': 'viewer_must_not_write'})
          assert (await packet(second, 'input_denied'))['reason'] == 'viewer only'
          await second.send_json({'type': 'raw', 'data': 'viewer_must_not_write'})
          assert (await packet(second, 'input_denied'))['reason'] == 'viewer only'
          await first.send_json({'type': 'input', 'data': "printf 'AUTO_RUN\\n'"})
          await packet(first, 'command_approved')
          await output(first, b'AUTO_RUN\r\n')
          await packet(owner, 'command_auto_run')
          await first.send_json({'type': 'raw', 'data': "printf 'RAW_RUN\\n'\r"})
          await output(first, b'RAW_RUN\r\n')
          await owner.send_json({'type': 'typing', 'active': 1, 'text': 'a\x00\x1b\nb' + 'x' * 200})
          typing = await packet(second, 'host_typing')
          assert typing['text'] == 'ab' + 'x' * 158
          await second.send_json({'type': 'typing', 'active': 1, 'text': 'g\x00\n'})
          assert (await packet(owner, 'guest_typing'))['text'] == 'g'
          await first.close()
          while not (await packet(second, 'control_role'))['granted']:
            continue
          await second.send_json({'type': 'input', 'data': "printf 'PROMOTED_RUN\\n'"})
          await packet(second, 'command_approved')
          await output(second, b'PROMOTED_RUN\r\n')
          await packet(owner, 'command_auto_run')
        await second.send_json({'type': 'raw', 'data': 'owner_absent'})
        await packet(second, 'owner_absent')
    remaining = []
    for count in [2, 3]:
      async with client.ws_connect(endpoint) as bad:
        for _ in range(count):
          await bad.send_json({'type': 'auth', 'pin': 'invalid'})
          remaining.append((await packet(bad, 'auth_failed'))['remaining'])
        if count == 3:
          message = await bad.receive()
          assert message.type == WSMsgType.CLOSE and message.data == 1000
    assert remaining == [4, 3, 2, 1, 0]
    async with client.ws_connect(endpoint) as valid:
      await valid.send_json({'type': 'auth', 'pin': pin})
      await packet(valid, 'auth_ok')
      await packet(valid, 'meta')
      await valid.send_json({'type': 'close_session'})
      await packet(valid, 'session_closed')
    async with client.get(fixture.url + '/api/support_terminal/status') as response:
      assert await response.json() == {'ok': True, 'active': False, 'state': 'idle'}
  await fixture.record('roles-result.json', {'auto_run': True, 'raw_run': True, 'viewer_denied': True, 'owner_required': True, 'typing_sanitized': True, 'promotion': True, 'shared_pin_remaining': remaining, 'correct_pin_after_limit': True, 'guest_close_session': True})


async def primary(fixture: Fixture) -> None:
  async with ClientSession() as client:
    async with client.ws_connect(fixture.url + '/ws/terminal_pty') as local:
      initial = await local.receive_json()
      assert initial['primary']
      async with client.post(fixture.url + '/api/support_terminal/start', json={}) as response:
        session = await response.json()
        assert response.status == 200
      origin = session['local_url'].split('/support/terminal/')[0]
      async with client.ws_connect(origin + '/ws/support_terminal/' + session['id']) as guest:
        await guest.send_json({'type': 'auth', 'pin': fixture.pin()})
        await packet(guest, 'auth_ok')
        assert not (await packet(guest, 'meta'))['primary']
        await local.close()
        with anyio.fail_after(2):
          while True:
            async with client.get(fixture.url + '/api/terminal_pty/status') as response:
              snapshot = await response.json()
            if snapshot['clients'] == 1:
              break
            await anyio.sleep(0.01)
        await fixture.record('primary-disconnect.json', snapshot)
        assert snapshot['primary']
        async with client.ws_connect(fixture.url + '/ws/terminal_pty') as replacement:
          replacement_meta = await replacement.receive_json()
          assert not replacement_meta['primary'] and replacement_meta['pid'] == initial['pid']
          await replacement.send_json({'type': 'input', 'data': "printf 'LOCAL_RECLAIMED\\n'"})
          await output(replacement, b'LOCAL_RECLAIMED\r\n')
        await fixture.record('primary-result.json', {'guest_promoted_on_disconnect': True, 'replacement_secondary': True, 'local_input_retained': True, 'same_pty': True})
    async with client.post(fixture.url + '/api/support_terminal/stop') as response:
      assert response.status == 200


async def expiry(fixture: Fixture) -> None:
  async with ClientSession() as client:
    began = time.monotonic()
    async with client.post(fixture.url + '/api/support_terminal/start', json={'ttl_seconds': 900}) as response:
      session = await response.json()
      assert response.status == 200 and session['ttl_seconds'] == 900
    updates = 0
    async with client.ws_connect(fixture.url + '/ws/support_terminal/owner') as owner:
      with anyio.fail_after(910):
        while True:
          message = await owner.receive_json()
          if message['type'] == 'session_status':
            updates += 1
          if message['type'] == 'session_closed':
            assert message['reason'] == 'expired'
            break
    elapsed = time.monotonic() - began
    assert 899 <= elapsed <= 905
    async with client.get(fixture.url + '/api/support_terminal/status') as response:
      assert (await response.json())['state'] == 'idle'
    await fixture.record('session-expiry-result.json', {'seconds': elapsed, 'status_updates': updates, 'reason': 'expired', 'state': 'idle'})
