"""Verify the native detector archive and the sole authorized manifest compatibility edit."""

import argparse
import hashlib
import json
from pathlib import Path
import tarfile


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('archive', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[1] / 'vendor'
  provenance = json.loads((root / 'charset-norm-provenance.json').read_text())
  checksum = hashlib.sha256(args.archive.read_bytes()).hexdigest()
  assert checksum == provenance['archive_sha256']
  package = root / 'charset-norm'
  files, changed = {}, []
  with tarfile.open(args.archive) as archive:
    for member in archive.getmembers():
      relative = Path(member.name).relative_to('charset-norm-3.5.1')
      assert not relative.is_absolute() and '..' not in relative.parts
      if member.isdir():
        continue
      assert member.isfile()
      original = archive.extractfile(member).read()
      actual = (package / relative).read_bytes()
      expected = original
      if relative == Path('Cargo.toml'):
        expected = original.replace(b'rust-version = "1.98"', b'rust-version = "1.94"')
      assert actual == expected, relative
      if actual != original:
        changed.append(str(relative))
      files[str(relative)] = hashlib.sha256(actual).hexdigest()
  assert changed == ['Cargo.toml']
  assert set(files) == {str(path.relative_to(package)) for path in package.rglob('*') if path.is_file()}
  args.output.write_text(json.dumps({'result': 'PASS', 'provenance': provenance, 'changed_files': changed, 'files': files}, indent=2))
  print(f'{len(files)} package files verified against published archive; only Cargo.toml rust-version changed')


if __name__ == '__main__':
  main()
