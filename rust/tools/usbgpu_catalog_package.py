"""Build immutable native companions from a catalog-bound HCQ artifact and pinned sources."""

from __future__ import annotations

from dataclasses import dataclass
import ast
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import tarfile
import tempfile

from import_usbgpu_hcq import Exporter
from usbgpu_hcq_data import Artifact, ArtifactError
from usbgpu_worker_metadata import export as worker_metadata


@dataclass(frozen=True, slots=True)
class Sources:
  catalog: Path
  model: Path
  runtime: Path
  hardware: Path
  selected_sha256: str
  warp_reference: Path | None = None


def fingerprint(path: Path) -> str:
  with path.open('rb') as source:
    return hashlib.file_digest(source, 'sha256').hexdigest()


def digest(value: object) -> bool:
  return isinstance(value, str) and len(value) == 64 and all(char in '0123456789abcdef' for char in value)


def catalog(sources: Sources) -> dict:
  if sources.catalog.stat().st_size > 65536:
    raise ArtifactError('catalog exceeds native size limit')
  value = json.loads(sources.catalog.read_text())
  if not isinstance(value, dict) or not digest(sources.selected_sha256):
    raise ArtifactError('selected model SHA-256 and catalog object required')
  if value.get('format') == 'comma-run-model':
    raise ArtifactError('comma-run-model requires its matching run_model artifact and pinned model_runtime.py queue contract')
  if value.get('format') != 'comma-generic-onnx':
    raise ArtifactError('unsupported catalog execution format')
  if (
    value.get('protocol') != 1
    or value.get('gpu_arch') != 'gfx1200'
    or value.get('frame_skip') != 4
    or value.get('camera_resolutions') != [[1928, 1208], [1344, 760]]
    or value.get('model_sha256') != sources.selected_sha256
  ):
    raise ArtifactError('catalog execution/input identity mismatch')
  for name, path, maximum in [('pickle', sources.model, 4 << 30), ('runtime', sources.runtime, 128 << 20)]:
    entry = value.get(name)
    if (
      not isinstance(entry, dict)
      or not digest(entry.get('sha256'))
      or type(entry.get('size')) is not int
      or not 0 < entry['size'] <= maximum
      or path.stat().st_size != entry['size']
      or fingerprint(path) != entry['sha256']
    ):
      raise ArtifactError(f'catalog {name} size/checksum mismatch')
  if value['pickle']['sha256'] != sources.selected_sha256:
    raise ArtifactError('generic pickle differs from selected model identity')
  return value


def runtime_sources(archive: Path, destination: Path) -> None:
  required = {'tinygrad/uop/__init__.py', 'examples/openpilot/compile_warp.py'}
  found: set[str] = set()
  total = 0
  with tarfile.open(archive, 'r:gz') as source:
    for index, member in enumerate(source):
      path = PurePosixPath(member.name)
      total += member.size
      if index >= 4096 or total > 128 << 20 or member.size < 0 or path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir()):
        raise ArtifactError('unsupported pinned runtime archive member')
      name = str(path)
      if name not in required:
        continue
      if name in found or not member.isfile() or member.size > 1 << 20:
        raise ArtifactError('invalid or duplicate pinned runtime source')
      stream = source.extractfile(member)
      if stream is None:
        raise ArtifactError('pinned runtime source unreadable')
      output = destination / name
      output.parent.mkdir(parents=True, exist_ok=True)
      with stream, output.open('wb') as target:
        shutil.copyfileobj(stream, target)
      found.add(name)
  if found != required:
    raise ArtifactError('pinned runtime lacks HCQ operation/warp source contract')


def warp_equivalence(root: Path, runtime: Path, reference: Path | None) -> dict | None:
  if reference is None:
    return None
  provenance = json.loads((root / 'source-provenance.json').read_text())
  if fingerprint(reference) != provenance.get('runtime_sha256'):
    raise ArtifactError('warp reference archive differs from hardware provenance')
  with tempfile.TemporaryDirectory(prefix='warp-reference-') as temporary:
    directory = Path(temporary)
    runtime_sources(reference, directory)
    before = (directory / 'examples/openpilot/compile_warp.py').read_bytes()
  after = (runtime / 'examples/openpilot/compile_warp.py').read_bytes()
  names = ['NV12Frame', 'warp_perspective_tinygrad', 'frames_to_tensor', 'make_frame_prepare', 'make_luma_warp']
  def functions(source):
    return {node.name: ast.dump(node, include_attributes=False) for node in ast.parse(source).body
            if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names}
  old, new = functions(before), functions(after)
  if set(old) != set(names) or old != new:
    raise ArtifactError('warp mathematical functions differ from verified companions')
  return {'reference_runtime_sha256':fingerprint(reference),
          'reference_source_sha256':hashlib.sha256(before).hexdigest(),
          'selected_source_sha256':hashlib.sha256(after).hexdigest(),
          'identical_ast_definitions':names,
          'scope':'NV12 warp math only; compiled kernels retained from verified reference package'}


def hardware_files(root: Path, runtime: Path, equivalence: dict | None = None) -> list[tuple[Path, str]]:
  manifest = json.loads((root / 'manifest.json').read_text())
  if manifest.get('version') != 1 or not isinstance(manifest.get('files'), list):
    raise ArtifactError('hardware companion manifest required')
  files: dict[str, Path] = {}
  total = 0
  for entry in manifest['files']:
    name = entry['path']
    relative = PurePosixPath(name)
    if not name or relative.is_absolute() or '..' in relative.parts or name in files:
      raise ArtifactError('invalid/duplicate hardware companion path')
    path = root
    for part in relative.parts:
      path = path / part
      if path.is_symlink():
        raise ArtifactError('hardware companion symlink rejected')
    if not path.is_file() or path.stat().st_size != entry['bytes'] or fingerprint(path) != entry['sha256']:
      raise ArtifactError(f'hardware companion checksum mismatch: {name}')
    total += entry['bytes']
    if total > 128 << 20:
      raise ArtifactError('hardware companion size limit')
    files[name] = path
  required = {'tinygrad-LICENSE', 'probe-gfx1200.json'}
  source_sha256 = fingerprint(runtime / 'examples/openpilot/compile_warp.py')
  allowed_sources = {source_sha256}
  if equivalence is not None:
    if equivalence['selected_source_sha256'] != source_sha256:
      raise ArtifactError('warp source changed after equivalence verification')
    allowed_sources.add(equivalence['reference_source_sha256'])
  for width, height in [(1344, 760), (1928, 1208)]:
    name = f'warp-gfx1200-{width}x{height}.json'
    required.add(name)
    if name not in files or json.loads(files[name].read_text()).get('source_sha256') not in allowed_sources:
      raise ArtifactError('AMD warp differs from pinned runtime source')
    qcom = f'warp-qcom-a630-{width}x{height}'
    required.update(f'{qcom}/{name}' for name in ['graph.json', 'weights.bin', 'provenance.json'])
    if f'{qcom}/provenance.json' not in files or json.loads(files[f'{qcom}/provenance.json'].read_text()).get('source_sha256') not in allowed_sources:
      raise ArtifactError('QCOM warp differs from pinned runtime source')
  if not required.issubset(files) or not any(name.startswith('firmware/') for name in files):
    raise ArtifactError('incomplete native hardware companions')
  return [(path, name) for name, path in sorted(files.items()) if not name.startswith(('models/', 'local/'))]


def publish(sources: Sources, destination: Path) -> Path:
  selected = catalog(sources)
  destination.parent.mkdir(parents=True, exist_ok=True)
  growth = (128 << 20) + sources.model.stat().st_size
  if shutil.disk_usage(destination.parent).free < (25 << 30) + growth:
    raise ArtifactError('native catalog export requires 25 GiB free plus estimated growth')
  with tempfile.TemporaryDirectory(prefix='.catalog-export-', dir=destination.parent) as temporary:
    temporary = Path(temporary)
    runtime = temporary / 'runtime'
    runtime_sources(sources.runtime, runtime)
    equivalence = warp_equivalence(sources.hardware, runtime, sources.warp_reference)
    hardware = hardware_files(sources.hardware, runtime, equivalence)
    stage = temporary / 'package'
    stage.mkdir()
    with Artifact(sources.model, runtime) as artifact:
      if artifact.sha256 != selected['pickle']['sha256']:
        raise ArtifactError('captured model changed after catalog verification')
      descriptor = Exporter(artifact).export()
      metadata = worker_metadata(artifact)
    if metadata['checkpoint'] != selected.get('model_checkpoint'):
      raise ArtifactError('captured model checkpoint differs from catalog')
    model = stage / 'models' / sources.selected_sha256
    model.mkdir(parents=True)
    for name, value in [('model.hcq.json', descriptor), ('model.hcq-meta.json', metadata)]:
      (model / name).write_text(json.dumps(value, separators=(',', ':')) + '\n')
    for source, name in hardware:
      target = stage / name
      target.parent.mkdir(parents=True, exist_ok=True)
      shutil.copyfile(source, target)
    provenance = {
      'format': selected['format'],
      'catalog_sha256': fingerprint(sources.catalog),
      'pickle_sha256': selected['pickle']['sha256'],
      'runtime_sha256': selected['runtime']['sha256'],
      'hardware_manifest_sha256': fingerprint(sources.hardware / 'manifest.json'),
      'build_sources': {
        name: fingerprint(Path(__file__).with_name(name))
        for name in ['usbgpu_catalog_package.py', 'import_usbgpu_hcq.py', 'usbgpu_hcq_data.py', 'usbgpu_hcq_dispatch.py', 'usbgpu_hcq_dispatch2.py', 'usbgpu_hcq_export2.py', 'usbgpu_worker_metadata.py']
      },
      'warp_source_equivalence':equivalence,
      'execution_boundary': 'Build-time data-only conversion; native HCQ dispatcher and source-derived AMD/QCOM warp companions. No runtime Python.',
    }
    (stage / 'source-provenance.json').write_text(json.dumps(provenance, sort_keys=True, indent=2) + '\n')
    files = [
      {'path': str(path.relative_to(stage)), 'bytes': path.stat().st_size, 'sha256': fingerprint(path)} for path in sorted(stage.rglob('*')) if path.is_file()
    ]
    manifest = {'version': 1, 'model_sha256': sources.selected_sha256, 'files': files}
    serialized = json.dumps(manifest, sort_keys=True, indent=2) + '\n'
    (stage / 'manifest.json').write_text(serialized)
    if destination.exists():
      if (destination / 'manifest.json').read_text() != serialized:
        raise ArtifactError('existing native catalog package differs from immutable export')
      for entry in files:
        target = destination / entry['path']
        if not target.is_file() or target.is_symlink() or fingerprint(target) != entry['sha256']:
          raise ArtifactError('existing native catalog package changed')
    else:
      stage.rename(destination)
  return destination
