"""Execute hash-verified upstream planner helpers for #338 test comparisons."""
import hashlib
import json
from pathlib import Path
from types import ModuleType

FIXTURE = Path(__file__).resolve().parents[1] / 'upstream/planner-source.json'


def source(path):
    fixture = json.loads(FIXTURE.read_text())
    assert fixture['source_repo'] == 'https://github.com/ajouatom/openpilot'
    assert fixture['source_commit'] == 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
    record = fixture['files'][path]
    assert record.get('source_commit', fixture['source_commit']) in (
        fixture['source_commit'], '8cf5570f6962bd9cb17ec384bbfa12a3a167a5a8')
    data = record['content'].encode()
    assert hashlib.sha256(data).hexdigest() == record['sha256']
    assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == record['git_blob']
    return record['content']


def module(path):
    result = ModuleType('verified_planner_' + Path(path).stem)
    exec(compile(source(path), f'e8e152db:{path}', 'exec'), result.__dict__)
    return result
