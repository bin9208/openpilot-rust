from __future__ import annotations
import hashlib
import io
import json
from pathlib import Path
import tarfile

import pytest
from usbgpu_catalog_package import Sources, catalog, runtime_sources
from usbgpu_hcq_data import ArtifactError


def test_catalog_binds_both_source_inputs_and_selected_identity(tmp_path: Path) -> None:
  model, runtime, descriptor = [tmp_path / name for name in ['model', 'runtime', 'catalog.json']]
  model.write_bytes(b'owned model')
  runtime.write_bytes(b'owned runtime')
  selected = hashlib.sha256(model.read_bytes()).hexdigest()
  value = {
    'format': 'comma-generic-onnx',
    'protocol': 1,
    'gpu_arch': 'gfx1200',
    'frame_skip': 4,
    'camera_resolutions': [[1928, 1208], [1344, 760]],
    'model_sha256': selected,
    'pickle': {'sha256': selected, 'size': model.stat().st_size},
    'runtime': {'sha256': hashlib.sha256(runtime.read_bytes()).hexdigest(), 'size': runtime.stat().st_size},
  }
  descriptor.write_text(json.dumps(value))
  sources = Sources(descriptor, model, runtime, tmp_path, selected)
  assert catalog(sources) == value
  runtime.write_bytes(b'changed input')
  with pytest.raises(ArtifactError, match='runtime size/checksum'):
    catalog(sources)
  runtime.write_bytes(b'owned runtime')
  model.write_bytes(b'wrong model')
  with pytest.raises(ArtifactError, match='pickle size/checksum'):
    catalog(sources)
  model.write_bytes(b'owned model')
  with pytest.raises(ArtifactError, match='identity'):
    catalog(Sources(descriptor, model, runtime, tmp_path, 'f' * 64))
  value['format'] = 'comma-run-model'
  descriptor.write_text(json.dumps(value))
  with pytest.raises(ArtifactError, match='model_runtime.py queue contract'):
    catalog(sources)


@pytest.mark.parametrize('name,kind', [('escape', tarfile.SYMTYPE), ('../outside', tarfile.REGTYPE), ('duplicate', tarfile.REGTYPE)])
def test_pinned_archive_rejects_escape_links_and_duplicate_sources(tmp_path: Path, name: str, kind: bytes) -> None:
  archive = tmp_path / 'runtime.tar.gz'
  with tarfile.open(archive, 'w:gz') as target:
    for path in ['tinygrad/uop/__init__.py', 'examples/openpilot/compile_warp.py']:
      item = tarfile.TarInfo(path)
      item.size = 1
      target.addfile(item, io.BytesIO(b'x'))
    item = tarfile.TarInfo('tinygrad/uop/__init__.py' if name == 'duplicate' else name)
    item.type = kind
    item.linkname = '../outside'
    if kind == tarfile.REGTYPE:
      item.size = 1
      target.addfile(item, io.BytesIO(b'y'))
    else:
      target.addfile(item)
  with pytest.raises(ArtifactError, match='archive member|duplicate'):
    runtime_sources(archive, tmp_path / 'sources')
  assert not (tmp_path / 'outside').exists()
