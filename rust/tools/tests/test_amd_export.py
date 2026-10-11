from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest
from tinygrad import Tensor, TinyJit
from tinygrad.device import Device

from model_export.amd import export_amd
from model_export.pipeline import Stage, compose
from model_export.schema import Bindings, ExportError


@pytest.fixture(autouse=True)
def owned_null_storage(monkeypatch: pytest.MonkeyPatch) -> None:
  allocator = Device.default.allocator
  monkeypatch.setattr(allocator, '_alloc', lambda size, _options: memoryview(bytearray(size)))
  monkeypatch.setattr(allocator, '_offset', lambda buffer, size, offset: buffer[offset : offset + size])
  monkeypatch.setattr(allocator, '_copyin', lambda destination, source: destination.cast('B').__setitem__(slice(None), source.cast('B')))
  monkeypatch.setattr(allocator, '_copyout', lambda destination, source: destination.cast('B').__setitem__(slice(None), source.cast('B')))


def test_staged_amd_graph_preserves_shared_output_and_queue(tmp_path: Path) -> None:
  frame = Tensor(np.arange(16, dtype=np.float32)).realize()
  state = Tensor(np.zeros(16, dtype=np.float32)).realize()
  prepare = TinyJit(lambda frame: (frame * 2 + 3).realize())
  warped = prepare(frame)
  warped = prepare(frame)
  policy = TinyJit(lambda warped, state: state.assign(state + warped).realize())
  policy(warped, state)
  policy(warped, state)
  bindings = Bindings({'frame': frame, 'state': state}, {'warped': warped, 'state': state})
  stages = (Stage('prepare', prepare, {'0': frame}), Stage('policy', policy, {'0': warped, '1': state}))
  pipeline = compose(stages, bindings)
  export_amd(pipeline, bindings, tmp_path / 'bundle')
  graph = json.loads((tmp_path / 'bundle/graph.json').read_text())
  assert graph['backend'] == 'amd-elf' and graph['arch'] == 'gfx1200'
  assert [entry['name'] for entry in graph['entrypoints']] == ['prepare', 'policy']
  bridge = next(binding['view'] for binding in graph['outputs'] if binding['name'] == 'warped')
  policy_range = graph['entrypoints'][1]
  assert any(bridge in call.get('views', []) for call in graph['calls'][policy_range['start'] : policy_range['end']])
  assert (tmp_path / 'bundle/kernel-0.bin').read_bytes().startswith(b'\x7fELF')


def test_amd_export_rejects_host_graph(tmp_path: Path) -> None:
  frame = Tensor(np.arange(4, dtype=np.float32), device='CPU').realize()
  prepare = TinyJit(lambda frame: (frame + 1).realize())
  prepare(frame)
  output = prepare(frame)
  bindings = Bindings({'frame': frame}, {'output': output})
  pipeline = compose((Stage('prepare', prepare, {'0': frame}),), bindings)
  with pytest.raises(ExportError, match='AMD'):
    export_amd(pipeline, bindings, tmp_path / 'bundle')
