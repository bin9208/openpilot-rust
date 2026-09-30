from __future__ import annotations

import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
PROVENANCE = {'runtime_language', 'source_commit', 'source_tree'}


def records(path: Path) -> tuple[list[tuple[int, str]], dict]:
  path = path.resolve()
  route = next((path / 'params').glob('*/CurrentRoute')).read_text()
  rows = json.loads((path / 'diagnostics.json').read_text())
  assert rows, path
  messages = []
  context = rows[0]['ctx']
  for row in rows:
    assert set(row) == {'created', 'ctx', 'filename', 'funcname', 'levelnum', 'lineno', 'msg'}
    assert row['ctx'] == context
    assert isinstance(row['created'], float) and row['created'] > 0
    assert row['funcname'] and row['lineno'] > 0
    source = Path(row['filename'])
    if not source.is_absolute():
      source = ROOT / 'rust' / source
    assert source.is_file(), source
    assert row['lineno'] <= len(source.read_text().splitlines())
    messages.append((row['levelnum'], row['msg'].replace(str(path), '<root>').replace(route, '<route>')))
  return messages, context


def compare(source: Path, candidate: Path) -> dict:
  original, original_context = records(source)
  rust, rust_context = records(candidate)
  assert rust_context['runtime_language'] == 'rust'
  assert re.fullmatch(r'[0-9a-f]{40}', rust_context['source_commit'])
  assert rust_context['source_tree'] in {'clean', 'dirty'}
  assert {key: value for key, value in rust_context.items() if key not in PROVENANCE} == original_context
  assert original == rust, (source, original, rust)
  return {'records_per_process': len(original), 'ordered_levels_and_messages': 'exact',
          'context': 'exact except verified Rust provenance', 'callsite': 'actual source file, line and function'}


def compare_tree(output: Path) -> dict:
  results = {}
  for path in sorted(output.rglob('diagnostics.json')):
    relative = str(path.relative_to(output))
    if 'original' not in relative:
      continue
    candidate = output / relative.replace('original', 'rust')
    if candidate.exists():
      results[str(path.parent.relative_to(output))] = compare(path.parent, candidate.parent)
  assert results
  (output / 'diagnostic-comparison.json').write_text(json.dumps(results, indent=2) + '\n')
  return results
