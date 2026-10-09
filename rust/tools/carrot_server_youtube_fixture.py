# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Original/native YouTube peers with owned Params, files, endpoints and unconditional child cleanup."""

from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from dataclasses import dataclass
from typing import TypedDict, assert_never

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_upload import request, save
from carrot_server_dashcam_sync_probe import Json
from carrot_server_youtube_probe import Probe
from carrot_server_youtube_ingest import Ingest


@dataclass(frozen=True, slots=True)
class Provider:
  name: str
  binary: Path
  application: bool = False


class Peer:
  def __init__(self, provider: Provider, root: Path, probe: Probe | Ingest) -> None:
    self.name = provider.name
    self.root = root
    self.probe = probe
    self.params = root / 'owned-params'
    self.prefix = 'youtube-' + uuid.uuid4().hex
    self.argv = (
      [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_youtube_source.py'))] if provider.name == 'source' else [str(provider.binary)]
    )
    self.binary = provider.binary
    self.application = provider.application
    self.process: anyio.abc.Process | None = None
    self.log = None
    self.port = 0

  async def start(self, certificate: Path) -> None:
    await anyio.Path(self.params / self.prefix).mkdir(parents=True, exist_ok=True)
    environment = os.environ | {
      'PARAMS_ROOT': str(self.params),
      'OPENPILOT_PREFIX': self.prefix,
      'CARROT_DATA_DIR': str(self.root),
      'SSL_CERT_FILE': str(certificate),
    }
    payload = {
      'owned_root': str(self.root),
      'params_root': str(self.params),
      'prefix': self.prefix,
      'endpoint': self.probe.endpoint,
      'application': self.application,
    }
    self.log = await anyio.Path(self.root / 'process.log').open('wb')
    # Register process ownership immediately; the enclosing fixture cleans every partial start failure.
    self.process = await anyio.open_process(self.argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log.wrapped, env=environment)
    assert self.process.stdin and self.process.stdout
    await self.process.stdin.send((json.dumps(payload) + '\n').encode())
    reader = BufferedByteReceiveStream(self.process.stdout)
    with anyio.fail_after(6):
      ready = json.loads(await reader.receive_until(b'\n', 65536))
    self.port = ready['port']
    await anyio.to_thread.run_sync(
      save,
      self.root / 'invocation.json',
      {
        'argv': self.argv,
        'environment': {name: environment[name] for name in ['PARAMS_ROOT', 'OPENPILOT_PREFIX', 'CARROT_DATA_DIR', 'SSL_CERT_FILE', 'ORIGINAL_PARAMS_BINDING']},
        'input': payload,
        'ready': ready,
        'binary_sha256': hashlib.sha256(await anyio.Path(self.binary).read_bytes()).hexdigest(),
      },
    )

  async def close(self) -> list[str]:
    await anyio.Path(self.root).mkdir(parents=True, exist_ok=True)
    errors = []
    process = self.process
    if process:
      try:
        if process.returncode is None:
          assert process.stdin
          await process.stdin.send(b'\n')
          await process.stdin.aclose()
        with anyio.fail_after(12):
          await process.wait()
      except (anyio.BrokenResourceError, anyio.ClosedResourceError, BrokenPipeError, TimeoutError) as error:
        errors.append(str(error))
        if process.returncode is None:
          process.kill()
          with anyio.fail_after(3):
            await process.wait()
      finally:
        await process.aclose()
    if self.log:
      await self.log.aclose()
    await anyio.to_thread.run_sync(self.probe.close)
    await anyio.to_thread.run_sync(
      save,
      self.root / 'cleanup.json',
      {
        'exit': process.returncode if process else None,
        'process_reaped': process is None or process.returncode is not None,
        'probe_thread_exited': not self.probe.thread.is_alive(),
        'errors': errors,
        'probe_observations': self.probe.observations,
        'probe_shutdown_errors': self.probe.errors,
      },
    )
    return errors


class Observation(TypedDict):
  label: str
  source: Json
  native: Json
  equal: bool


def normalized(value: Json, root: Path, endpoint: str) -> Json:
  match value:
    case dict():
      return {key: normalized(item, root, endpoint) for key, item in value.items() if key not in ('generated_at', 'updated_at')}
    case list():
      return [normalized(item, root, endpoint) for item in value]
    case str():
      return value.replace(str(root), '$ROOT').replace(endpoint, '$ENDPOINT').replace('python-tls-tunnel', '$TLS-PROVIDER')
    case None | bool() | int() | float():
      return value
    case unexpected:
      assert_never(unexpected)


class Fixture:
  def __init__(self, output: Path, binary: Path, certificate: tuple[Path, Path]) -> None:
    self.output = output
    self.binary = binary
    self.certificate = certificate
    self.peers = [Peer(Provider(name, binary), output / name, Probe(*certificate)) for name in ['source', 'native']]
    self.rows: list[Observation] = []

  async def start(self) -> None:
    for peer in self.peers:
      await peer.start(self.certificate[0])

  async def pair(self, label: str, path: str, operation: tuple[str, bytes, int]) -> list[Json]:
    method, body, status = operation
    captured = [await request(peer.port, path, method, body) for peer in self.peers]
    for index, response in enumerate(captured):
      assert response['status'] == status, (label, index, response)
      raw = base64.b64decode(response['body_base64'])
      if method != 'HEAD':
        assert int(response['headers']['content-length']) == len(raw)
      await anyio.to_thread.run_sync(save, self.output / f'{label}-{self.peers[index].name}.json', response)
    values = [normalized(response['payload'], peer.root, peer.probe.endpoint) for peer, response in zip(self.peers, captured, strict=True)]
    for value in values:
      if isinstance(value, dict):
        diagnostics = value.get('diagnostics')
        if isinstance(diagnostics, dict):
          muxer, transport = diagnostics['muxer'], diagnostics['transport']
          muxer['version'] = '$FFMPEG-PROVIDER'
          transport['rtmps_mode'] = '$TLS-PROVIDER'
    for value in values:
      if isinstance(value, dict) and 'muxer' in value and isinstance(value['muxer'], dict):
        value['muxer']['version'] = '$FFMPEG-PROVIDER'
        if isinstance(value.get('transport'), dict):
          value['transport']['rtmps_mode'] = '$TLS-PROVIDER'
    equal = values[0] == values[1]
    self.rows.append({'label': label, 'source': values[0], 'native': values[1], 'equal': equal})
    await anyio.to_thread.run_sync(save, self.output / 'observations.json', self.rows)
    assert equal, (label, values)
    return values

  async def close(self) -> None:
    errors = []
    for peer in self.peers:
      try:
        errors.extend(await peer.close())
      except (OSError, TimeoutError, AssertionError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
        errors.append(str(error))
    assert not errors, errors
    assert all(peer.process and peer.process.returncode == 0 for peer in self.peers)
