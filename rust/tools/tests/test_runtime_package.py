import json

import pytest

from seal_runtime_package import BOOT_BINARIES, PackageError, seal


@pytest.fixture
def package(tmp_path):
  binary = tmp_path / 'rust/bin/owned-daemon'
  binary.parent.mkdir(parents=True)
  elf = bytearray(64)
  elf[:6] = b'\x7fELF\x02\x01'
  elf[18:20] = (183).to_bytes(2, 'little')
  binary.write_bytes(elf)
  binary.chmod(0o755)
  inventory = [{'name': 'owned', 'ready': True, 'argv': [str(binary)], 'requirements': [{'path': str(binary), 'executable': True}]}]
  return tmp_path, binary, inventory


def seal_fixture(package):
  root, _, inventory = package
  return seal(root, inventory, 'aarch64', '1' * 40, required_boot=(), required_companions=())


def test_manifest_covers_binary_and_companion_bytes(package):
  root, binary, inventory = package
  companion = binary.parent / 'owned-data'
  companion.mkdir()
  (companion / 'weights').write_bytes(b'owned weights')
  inventory[0]['requirements'].append({'path': str(companion), 'directory': True})
  manifest = seal_fixture(package)
  assert manifest['source_commit'] == '1' * 40
  assert {row['path'] for row in manifest['files']} == {'rust/bin/owned-daemon', 'rust/bin/owned-data/weights'}
  assert json.loads((root / 'rust/runtime-manifest.json').read_text()) == manifest


def test_accepts_manager_osstring_argument_encoding(package):
  _, binary, inventory = package
  inventory[0]['argv'] = [{'Unix': list(bytes(binary))}]
  manifest = seal_fixture(package)
  assert manifest['files'][0]['executable'] is True


@pytest.mark.parametrize('argument', [{'Unix': [256]}, {'Unix': [True]}, {'Unix': 'binary'}, {'Windows': []}, None])
def test_rejects_invalid_manager_argument_encoding(package, argument):
  root, _, inventory = package
  inventory[0]['argv'] = [argument]
  with pytest.raises(PackageError, match='argument encoding'):
    seal_fixture(package)
  assert not (root / 'rust/runtime-manifest.json').exists()


def test_required_source_asset_is_hashed_in_manifest(package):
  root, _, inventory = package
  firmware = root / 'panda/board/obj/panda.bin.signed'
  firmware.parent.mkdir(parents=True)
  firmware.write_bytes(b'owned firmware fixture')
  inventory[0]['requirements'].append({'path': str(firmware), 'file': True})
  manifest = seal_fixture(package)
  assets = {row['path']: row for row in manifest['files']}
  assert assets['panda/board/obj/panda.bin.signed']['executable'] is False
  assert len(assets['panda/board/obj/panda.bin.signed']['sha256']) == 64


@pytest.mark.parametrize('contents', [None, b'', b'version https://git-lfs.github.com/spec/v1\noid sha256:' + b'0' * 64 + b'\nsize 4\n'])
def test_unavailable_required_file_cannot_be_sealed(package, contents):
  root, _, inventory = package
  asset = root / 'panda/board/obj/panda.bin.signed'
  asset.parent.mkdir(parents=True)
  if contents is not None:
    asset.write_bytes(contents)
  inventory[0]['requirements'].append({'path': str(asset), 'file': True})
  with pytest.raises(PackageError, match='required file'):
    seal_fixture(package)
  assert not (root / 'rust/runtime-manifest.json').exists()


def test_bootstrap_gpu_companion_is_required_even_without_manager_binding(package):
  root, binary, inventory = package
  for present in (False, True):
    companion = binary.parent / 'usbgpu-assets'
    if present:
      companion.mkdir()
    with pytest.raises(PackageError, match='companion'):
      seal(root, inventory, 'aarch64', '1' * 40, required_boot=())
    assert not (root / 'rust/runtime-manifest.json').exists()
  (companion / 'owned-kernel').write_bytes(b'fixture kernel')
  manifest = seal(root, inventory, 'aarch64', '1' * 40, required_boot=())
  assert any(row['path'] == 'rust/bin/usbgpu-assets/owned-kernel' for row in manifest['files'])


@pytest.mark.parametrize('missing', ['openpilot-usbgpu-worker', 'openpilot-usbgpu-probe'])
def test_gpu_helpers_are_required_even_without_manager_binding(package, missing):
  root, binary, inventory = package
  for name in BOOT_BINARIES:
    if name != missing:
      helper = binary.parent / name
      helper.write_bytes(binary.read_bytes())
      helper.chmod(0o755)
  with pytest.raises(PackageError, match=missing):
    seal(root, inventory, 'aarch64', '1' * 40, required_companions=())


def test_missing_implementation_never_produces_candidate_manifest(package):
  root, _, inventory = package
  inventory.append({'name': 'carrot_cluster', 'enabled': True, 'ready': False, 'error': 'native implementation unavailable'})
  with pytest.raises(PackageError, match='carrot_cluster'):
    seal_fixture(package)
  assert not (root / 'rust/runtime-manifest.json').exists()


def test_wrong_architecture_and_script_are_rejected(package):
  _, binary, _ = package
  for content in (b'#!/bin/sh\nexit 0\n', b'\x7fELF\x02\x01' + bytes(12) + (62).to_bytes(2, 'little') + bytes(44)):
    binary.write_bytes(content)
    with pytest.raises(PackageError):
      seal_fixture(package)


def test_empty_companion_and_external_link_are_rejected(package):
  _, binary, inventory = package
  companion = binary.parent / 'owned-data'
  companion.mkdir()
  inventory[0]['requirements'].append({'path': str(companion), 'directory': True})
  with pytest.raises(PackageError, match='empty'):
    seal_fixture(package)
  (companion / 'outside').symlink_to('/etc/passwd')
  with pytest.raises(PackageError, match='escapes'):
    seal_fixture(package)


def test_absent_optional_webjoystick_is_explicitly_exempt(package):
  root, _, inventory = package
  inventory.append({'name': 'webjoystick', 'enabled': False, 'ready': False, 'error': 'native implementation unavailable'})
  seal_fixture(package)
  source = root / 'openpilot/tools/bodyteleop/web.py'
  source.parent.mkdir(parents=True)
  source.write_text('owned optional source')
  with pytest.raises(PackageError, match='webjoystick'):
    seal_fixture(package)


def test_directory_alias_is_rejected_before_publishing_but_file_alias_is_sealed(package):
  root, binary, inventory = package
  real = binary.parent / 'provider-real'
  real.mkdir()
  library = real / 'library.so'
  library.write_bytes(b'owned provider bytes')
  alias = binary.parent / 'provider-alias'
  alias.symlink_to(real.name, target_is_directory=True)
  inventory[0]['requirements'].append({'path': str(alias), 'directory': True})
  prior = root / 'rust/runtime-manifest.json'
  prior.write_text('prior sealed generation\n')
  with pytest.raises(PackageError, match='directory symlink'):
    seal_fixture(package)
  assert prior.read_text() == 'prior sealed generation\n'
  inventory[0]['requirements'].pop()
  with pytest.raises(PackageError, match='directory symlink'):
    seal_fixture(package)
  alias.unlink()
  file_alias = binary.parent / 'library-alias.so'
  file_alias.symlink_to('provider-real/library.so')
  manifest = seal_fixture(package)
  rows = {row['path']: row for row in manifest['files']}
  assert rows['rust/bin/library-alias.so']['sha256'] == rows['rust/bin/provider-real/library.so']['sha256']
  assert set(rows) == {'rust/bin/owned-daemon', 'rust/bin/library-alias.so', 'rust/bin/provider-real/library.so'}
  assert json.loads(prior.read_text()) == manifest
