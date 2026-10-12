from __future__ import annotations

import hashlib
import json
import math
import os
import shutil
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path

from tinygrad import Device, dtypes
from tinygrad.renderer.cstyle import QCOMCLRenderer

from openpilot.common.file_chunker import open_file_chunked
from openpilot.selfdrive.modeld.compile_modeld import get_policy_npy_shapes

from .amd import export_amd
from .original import driving
from .pipeline import Stage, compose
from .qcom import export_qcom
from .schema import Bindings, ExportError


@dataclass(frozen=True, slots=True)
class Scalar:
  name: str
  offset: int
  shape: tuple[int, ...]


@dataclass(frozen=True, slots=True)
class Camera:
  camera: tuple[int, int]
  graph: str
  qcom_warp: str | None


@dataclass(frozen=True, slots=True)
class File:
  path: str
  bytes: int
  sha256: str


@dataclass(frozen=True, slots=True)
class Manifest:
  version: int
  format: str
  model_sha256: str
  pickle_sha256: str
  pickle_bytes: int
  filename: str
  checkpoint: str
  scalar_inputs: tuple[Scalar, ...]
  output_count: int
  output_slices: dict[str, tuple[int, int]]
  cameras: tuple[Camera, ...]
  files: tuple[File, ...]


@dataclass(frozen=True, slots=True)
class Sources:
  model: Path
  selected_sha256: str
  assets: Path


def fingerprint(path: Path) -> tuple[int, str]:
  size, digest = 0, hashlib.sha256()
  with open_file_chunked(str(path)) as source:
    while data := source.read(256 << 10):
      size += len(data)
      if size > 4 << 30:
        raise ExportError('local source exceeds native artifact limit')
      digest.update(data)
  return size, digest.hexdigest()


def emit(source: Path, camera: tuple[int, int], destination: Path) -> Camera:
  original = driving(source, camera)
  prepare, policy = original.stages
  graph = f'amd-{camera[0]}x{camera[1]}'
  warp = None
  device = prepare.inputs['frame'].device
  backend = 'QCOM' if isinstance(Device[device].renderer, QCOMCLRenderer) else device.split(':')[0]
  match backend:  # MATCH_OK: Unsupported external build backends are explicitly rejected.
    case 'AMD' | 'NULL':
      export_amd(original.pipeline, original.bindings, destination / graph)
    case 'QCOM':
      warp = f'qcom-{camera[0]}x{camera[1]}'
      prepare_bindings = Bindings(prepare.inputs, {'warped': original.bindings.outputs['warped']})
      prepared = compose((prepare,), prepare_bindings)
      export_qcom(prepared.jit, prepare_bindings, destination / warp, entrypoints=prepared.entrypoints)
      policy_bindings = Bindings(policy.inputs, {name: tensor for name, tensor in original.bindings.outputs.items() if name != 'warped'})
      selected = compose((Stage('policy', policy.jit, policy.inputs),), policy_bindings)
      export_amd(selected, policy_bindings, destination / graph)
    case _:
      raise ExportError('native local export requires captured AMD or QCOM warp and AMD policy')
  return Camera(camera, graph, warp)


def publish(sources: Sources, root: Path) -> Path:
  digest = sources.selected_sha256
  if len(digest) != 64 or any(value not in '0123456789abcdef' for value in digest):
    raise ExportError('selected ONNX SHA-256 required')
  name = f'big_driving_{digest[:16]}_tinygrad.pkl'
  if sources.model.name != name:
    raise ExportError('compiled filename must match selected ONNX identity')
  pickle_bytes, pickle_digest = fingerprint(sources.model)
  original = driving(sources.model, (1344, 760))
  shapes, sizes = get_policy_npy_shapes(original.metadata.input_shapes)
  if tuple(shapes) != ('desire', 'traffic_convention', 'action_t', 'prev_feat'):
    raise ExportError('unsupported source packed policy contract')
  offset, scalars = 0, []
  for (key, shape), size in zip(shapes.items(), sizes, strict=True):
    scalars.append(Scalar(key, offset, tuple(shape)))
    offset += size * 4
  packed = original.bindings.inputs['packed_npy_inputs']
  if packed.dtype != dtypes.float32 or packed.numel() * packed.dtype.itemsize != offset:
    raise ExportError('packed source layout disagrees with captured policy')
  growth = 2 * pickle_bytes + (128 << 20)
  root.mkdir(parents=True, exist_ok=True)
  if shutil.disk_usage(root).free < (25 << 30) + growth:
    raise ExportError('native local export requires 25 GiB free plus estimated package growth')
  model_root = root / 'local' / name
  destination = model_root / pickle_digest
  destination.parent.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='.local-export-', dir=destination.parent) as temporary:
    stage = Path(temporary) / 'package'
    stage.mkdir()
    cameras = tuple(emit(sources.model, camera, stage) for camera in ((1928, 1208), (1344, 760)))
    for relative in ('tinygrad-LICENSE', 'probe-gfx1200.json'):
      shutil.copyfile(sources.assets / relative, stage / relative)
    shutil.copytree(sources.assets / 'firmware', stage / 'firmware')
    files = tuple(
      File(str(path.relative_to(stage)), path.stat().st_size, hashlib.sha256(path.read_bytes()).hexdigest())
      for path in sorted(stage.rglob('*'))
      if path.is_file()
    )
    metadata = original.metadata
    manifest = Manifest(
      1,
      'run-policy',
      digest,
      pickle_digest,
      pickle_bytes,
      name,
      metadata.model_checkpoint,
      tuple(scalars),
      math.prod(next(iter(metadata.output_shapes.values()))),
      dict(metadata.output_slices),
      cameras,
      files,
    )
    serialized = (json.dumps(asdict(manifest), sort_keys=True, indent=2) + '\n').encode()
    (stage / 'manifest.json').write_bytes(serialized)
    if destination.exists():
      if (destination / 'manifest.json').read_bytes() != serialized:
        raise ExportError('existing native local package differs from immutable export')
      for entry in files:
        path = destination / entry.path
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != entry.sha256:
          raise ExportError('existing native local package data differs from manifest')
    else:
      stage.rename(destination)
    pointer = Path(temporary) / 'current.json'
    with pointer.open('w') as output:
      json.dump({'version': 1, 'pickle_sha256': pickle_digest}, output)
      output.write('\n')
      output.flush()
      os.fsync(output.fileno())
    os.replace(pointer, model_root / 'current.json')
  return destination
