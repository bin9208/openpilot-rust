# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Route/coercion/actual remote-address observations for the whole Navi fixture.
from __future__ import annotations

import base64
import json
from pathlib import Path

from carrot_server_dashcam_sync_probe import Json
from carrot_server_dashcam_upload import save
from carrot_server_navi_fixture import Fixture
from carrot_server_navi_wire import fetch, payload, stable


def equal(left: Json, right: Json) -> bool:
  return json.dumps(stable(left), sort_keys=True) == json.dumps(stable(right), sort_keys=True)


async def routes(owned: Fixture, output: Path) -> None:
  rows = []
  cases = [
    ('GET', '/api/carrot_navi/capabilities', b'', 200),
    ('HEAD', '/api/carrot_navi/capabilities', b'', 200),
    ('GET', '/api/carrot_navi/status', b'', 200),
    ('HEAD', '/api/carrot_navi/status', b'', 200),
    ('POST', '/api/carrot_navi/status', b'', 405),
    ('GET', '/api/carrot_navi/client_diagnostic', b'', 405),
    ('POST', '/api/carrot_navi/client_diagnostic', b'{', 400),
    ('POST', '/api/carrot_navi/client_diagnostic', b'[]', 400),
    ('POST', '/api/carrot_navi/client_diagnostic', b'X' * 8193, 413),
    ('POST', '/api/carrot_navi/client_diagnostic', '{"unicode":"owned 한글"}'.encode('utf-16'), 200),
    ('POST', '/api/carrot_navi/client_diagnostic', b'{"surrogate":"\xed\xa0\x80"}', 200),
  ]
  for method, path, body, status in cases:
    responses = [await fetch(peer, path, method, body) for peer in owned.peers]
    rows.append({'method': method, 'path': path, 'body_base64': base64.b64encode(body).decode(), 'responses': responses})
    save(output / 'http.json', rows)
    assert all(row['status'] == status for row in responses)
    if status == 200 and method != 'HEAD':
      assert equal(payload(responses[0]), payload(responses[1])), responses
    else:
      assert responses[0]['body_base64'] == responses[1]['body_base64'], responses
  for local, label in [('127.0.0.2', 'reverse-first'), ('127.0.0.1', 'lexical-first'), ('127.0.0.2', 'updated')]:
    responses = [await fetch(peer, '/api/carrot_navi/client_diagnostic', 'POST', json.dumps({'label': label}).encode(), local) for peer in owned.peers]
    assert all(row['status'] == 200 for row in responses)
    rows.append({'actual_local_ip': local, 'diagnostic': label, 'responses': responses})
  responses = [await fetch(peer, '/api/carrot_navi/status') for peer in owned.peers]
  values = [payload(row) for row in responses]
  save(output / 'diagnostic-order.json', {'responses': responses})
  assert equal(values[0], values[1]), values
  assert [(row['peer'], row['label']) for row in values[0]['clientDiagnostics']] == [('127.0.0.1', 'lexical-first'), ('127.0.0.2', 'updated')]
  save(output / 'http.json', rows)
