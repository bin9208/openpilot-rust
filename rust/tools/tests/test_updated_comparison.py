"""Keep the updater's single intentional lifecycle difference narrowly scoped."""
import copy
import os

import pytest

from check_updated import assert_equivalent, assert_preserved


def traces():
  source = {
    'snapshots': [{'consistent': True, 'params': {'UpdateAvailable': '31'}, 'wait': 5400}],
    'agnos': [],
    'commands': [
      {'argv': ['sudo', 'rm', '-rf', '<ROOT>/data/stage'], 'cwd': None},
      {'argv': ['git', 'fetch', 'origin'], 'cwd': '<ROOT>/data/stage/merged'},
    ],
  }
  native = copy.deepcopy(source)
  native['commands'].pop(0)
  return source, native


def test_only_exact_obsolete_staging_root_removal_is_allowed():
  source, native = traces()
  assert assert_equivalent(source, native) == 1


@pytest.mark.parametrize('change', ['snapshot', 'params', 'agnos', 'command', 'cwd', 'extra', 'unknown'])
def test_unchanged_fields_and_remaining_command_order_stay_strict(change):
  source, native = traces()
  if change == 'snapshot':
    native['snapshots'][0]['consistent'] = False
  elif change == 'params':
    native['snapshots'][0]['params']['UpdateAvailable'] = '30'
  elif change == 'agnos':
    native['agnos'] = [{'kind': 'flash'}]
  elif change == 'command':
    native['commands'][0]['argv'].append('--force')
  elif change == 'cwd':
    native['commands'][0]['cwd'] = '<ROOT>/base'
  elif change == 'extra':
    native['commands'].append(copy.deepcopy(native['commands'][0]))
  else:
    native['new_field'] = True
  with pytest.raises(AssertionError):
    assert_equivalent(source, native)


@pytest.mark.parametrize('change', ['native-rm', 'wrong-root', 'wrong-cwd', 'extra-source-command'])
def test_destructive_native_command_and_other_source_removals_are_rejected(change):
  source, native = traces()
  if change == 'native-rm':
    native['commands'].insert(0, copy.deepcopy(source['commands'][0]))
  elif change == 'wrong-root':
    source['commands'][0]['argv'][-1] = '<ROOT>/data/stage/old_openpilot'
  elif change == 'wrong-cwd':
    source['commands'][0]['cwd'] = '<ROOT>/base'
  else:
    source['commands'].append({'argv': ['git', 'reset', '--hard'], 'cwd': '<ROOT>/base'})
  with pytest.raises(AssertionError):
    assert_equivalent(source, native)


@pytest.mark.parametrize('change', [None, 'rollback', 'lock-bytes', 'lock-inode'])
def test_rollback_and_lock_bytes_and_inode_are_independently_required(change, tmp_path):
  staging = tmp_path
  rollback = staging / 'old_openpilot.fixture/owned-rollback'
  rollback.parent.mkdir()
  rollback.write_bytes(b'complete previous source/package pair\n')
  lock = staging / '.runtime-update.lock'
  lock.write_bytes(b'owned shared transaction lock\n')
  # Keep the old inode alive, so removal/recreation cannot reuse it.
  with lock.open('rb') as original:
    inode = os.fstat(original.fileno()).st_ino
    if change == 'rollback':
      rollback.write_bytes(b'changed rollback')
    elif change == 'lock-bytes':
      lock.write_bytes(b'changed lock')
    elif change == 'lock-inode':
      lock.unlink()
      lock.write_bytes(b'owned shared transaction lock\n')
    if change is None:
      assert_preserved(staging, inode)
    else:
      with pytest.raises(AssertionError):
        assert_preserved(staging, inode)
