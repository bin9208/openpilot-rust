#!/usr/bin/env python3
"""Actions-only admission of pinned ARM packages in an owned SDK prefix.

No package installation, maintainer scripts, guest APT or system source changes.
Production SDK/bootstrap orchestration remains separate from this CI evidence.
"""
import email.parser
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess


def digest(path):
  with path.open('rb') as stream:
    return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
  assert os.environ['RUNNER_NAME'] == 'bin-actions' and os.environ['RUNNER_ENVIRONMENT'] == 'self-hosted'
  owned = Path(os.environ['CI_JOB_TEMP'])
  assert owned.parent == Path('/tmp') and owned.name.startswith('openpilot-ci.') and not owned.is_symlink()
  assert (owned/'owner').read_text() == ':'.join(os.environ[k] for k in ('GITHUB_RUN_ID','GITHUB_RUN_ATTEMPT','GITHUB_JOB'))
  source = Path(os.environ['GITHUB_WORKSPACE'])
  assert subprocess.check_output(['git','-C',source,'rev-parse','HEAD'],text=True).strip() == os.environ['GITHUB_SHA']
  plan = json.loads((source/'rust/native-capnp-assets.json').read_text())
  first = json.loads((owned/'native-capnp-assets-first.json').read_text())
  reused = json.loads((owned/'native-capnp-assets-reused.json').read_text())
  assert first.keys() == reused.keys() == plan['assets'].keys()
  expected = {'capnproto-1-0-1-4-arm64':'capnproto',
              'libcapnp-1-0-1-1-0-1-4-arm64':'libcapnp-1.0.1',
              'libcapnp-dev-1-0-1-4-arm64':'libcapnp-dev'}
  assert expected.keys() == first.keys()
  available = shutil.disk_usage(owned).free
  growth = 64*1024**2
  print(json.dumps(dict(path=str(owned),available_bytes=available,expected_unpack_growth_bytes=growth)),flush=True)
  if available < growth:
    raise RuntimeError('insufficient capacity for this owned package extraction; other data retained')
  prefix = owned/'native-capnp'
  prefix.mkdir()
  packages = {}
  tool = Path('/usr/bin/dpkg-deb')
  tool_sha = digest(tool)
  for name, asset in plan['assets'].items():
    receipt = first[name]
    blob = Path(receipt['path'])
    assert not blob.is_symlink() and blob.is_file()
    assert blob.resolve().is_relative_to(owned/'native-capnp-assets')
    assert blob.stat().st_size == receipt['bytes'] == asset['bytes']
    assert digest(blob) == receipt['sha256'] == asset['sha256']
    assert reused[name]['path'] == receipt['path'] and reused[name]['transferred_bytes'] == reused[name]['stored_bytes'] == 0
    control = subprocess.check_output([tool,'--field',blob],text=True)
    assert len(control) <= 16384
    fields = email.parser.Parser().parsestr(control)
    assert fields['Package'] == expected[name] and fields['Version'] == '1.0.1-4' and fields['Architecture'] == 'arm64'
    subprocess.run([tool,'--extract',blob,prefix],check=True)
    assert digest(blob) == asset['sha256']
    packages[name] = dict(package=fields['Package'],version=fields['Version'],architecture=fields['Architecture'],
                          sha256=asset['sha256'],bytes=asset['bytes'],depends=fields.get('Depends'))
  executable = prefix/'usr/bin/capnp'
  with executable.open('rb') as stream:
    header = stream.read(64)
  assert len(header) == 64 and header[:7] == b'\x7fELF\x02\x01\x01' and int.from_bytes(header[18:20],'little') == 183
  assert (prefix/'usr/include/capnp/message.h').is_file()
  assert all((prefix/'usr/share/doc'/package/'copyright').is_file() for package in expected.values())
  assert digest(tool) == tool_sha
  provenance = dict(schema=1,source_commit=os.environ['GITHUB_SHA'],packages=packages,
                    preparation_tool=str(tool),preparation_tool_sha256=tool_sha,
                    executable_sha256=digest(executable),compiler_executed=False,system_packages_installed=False)
  # Compiler-input identity follows the vendor content. Project commits and
  # host extraction-tool identity belong in the separate execution evidence.
  intrinsic = dict(schema=1,distribution='Ubuntu Noble',architecture='arm64',packages=packages)
  (prefix/'native-capnp-provenance.json').write_text(json.dumps(intrinsic,indent=2)+'\n')
  for path in prefix.rglob('*'):
    mode = path.lstat().st_mode
    if stat.S_ISREG(mode):
      path.chmod(0o555 if mode & 0o111 else 0o444)
  (owned/'native-capnp-preparation.json').write_text(json.dumps(provenance,indent=2)+'\n')
  print(json.dumps(provenance),flush=True)


if __name__ == '__main__':
  main()
