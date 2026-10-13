# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Caller-owned process/IPC namespace cleanup shared by Navi boundary controls.
from __future__ import annotations

from contextlib import asynccontextmanager
from dataclasses import dataclass
import os
from pathlib import Path
import sys
from collections.abc import AsyncIterator
import uuid

import anyio
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save


@dataclass(frozen=True, slots=True)
class Fixture:
  peers: tuple[Peer, Peer]
  params: tuple[Path, Path]
  namespace: str
  environment: dict[str, str]


@asynccontextmanager
async def fixture(
  binary: Path, output: Path, cluster: bytes = b'0', expected: int = 0, params: bool = True, unavailable: bool = False, composed: bool = False
) -> AsyncIterator[Fixture]:
  await anyio.Path(output).mkdir(parents=True)
  namespace = 'rust-navi-' + uuid.uuid4().hex
  shared = anyio.Path('/dev/shm', 'msgq_' + namespace)
  await shared.mkdir()
  root = output / 'params'
  values = tuple(root / name / namespace for name in ('source', 'native'))
  for path in values:
    await anyio.Path(path).mkdir(parents=True)
    await anyio.Path(path / 'ClusterHud').write_bytes(cluster)
  env = {**os.environ, 'OPENPILOT_PREFIX': namespace, 'PARAMS_ROOT': str(root), 'CARROT_DATA_DIR': str(output / 'data')}
  peers = tuple(Peer(output / name) for name in ('source', 'native'))
  commands = ([sys.executable, '-P', str(Path(__file__).with_name('carrot_server_navi_source.py'))], [str(binary)])
  try:
    for peer, command, params_path in zip(peers, commands, values, strict=True):
      await anyio.Path(peer.output).mkdir()
      await startup(
        peer,
        peer.start(
          command,
          {'params': params, 'unavailable': unavailable, 'composed': composed, 'output': str(peer.output)},
          {**env, 'PARAMS_ROOT': str(params_path.parent)},
          True,
        ),
      )
    yield Fixture(peers, values, namespace, env)
  finally:
    errors = []
    with anyio.CancelScope(shield=True):
      for peer in peers:
        try:
          await peer.close(expected)
        except (AssertionError, OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
          errors.append(f'{peer.output.name}: {type(error).__name__}: {error}')
      async for path in shared.iterdir():
        await path.unlink()
      await shared.rmdir()
    save(output / 'fixture-cleanup.json', {'errors': errors, 'namespace_removed': not await shared.exists()})
    assert not errors, errors
