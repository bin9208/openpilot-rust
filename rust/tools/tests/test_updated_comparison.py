"""Keep the updater's lifecycle and selected-remote differences narrowly scoped."""
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


def remote_traces():
  source, native = traces()
  cwd = '<ROOT>/data/stage/merged'

  def commands(arguments):
    return [{'argv': argv, 'cwd': cwd} for argv in arguments]

  selection = [
    ['git', 'rev-parse', '--abbrev-ref', 'HEAD'],
    ['git', 'config', '--get', 'branch.dev.remote'],
    ['git', 'remote', 'get-url', 'origin'],
  ]
  options = [
    ['git', 'config', 'core.trustctime', 'false'],
    ['git', 'config', 'core.checkStat', 'minimal'],
    ['git', 'config', 'protocol.version', '2'],
    ['git', 'config', 'gc.auto', '0'],
    ['git', 'config', 'gc.autoDetach', 'false'],
  ]
  probe = [['git', 'ls-remote', 'origin', 'HEAD']]
  fetch = [
    ['git', 'config', '--replace-all', 'remote.origin.fetch', '+refs/heads/*:refs/remotes/origin/*'],
    ['git', 'fetch', 'origin', 'dev'],
  ]
  source['commands'] = source['commands'][:1] + commands(probe + options + [['git', 'ls-remote', '--heads']] + options + fetch)
  native['commands'] = commands(selection + probe + options + [['git', 'ls-remote', '--heads', 'origin']] + selection + options + fetch)
  return source, native


def test_exact_selection_reads_and_explicit_origin_heads_are_allowed():
  source, native = remote_traces()
  assert assert_equivalent(source, native) == 1


@pytest.mark.parametrize('reset_overlay', [False, True])
def test_selection_exception_tracks_checkout_and_overlay_branch(reset_overlay):
  source, native = remote_traces()
  cwd = '<ROOT>/data/stage/merged'
  transition = [{'argv': ['git', 'checkout', '--force', '--no-recurse-submodules', '-B', 'release-tizi', 'FETCH_HEAD'], 'cwd': cwd}]
  if reset_overlay:
    transition.append({'argv': ['sudo', 'mount', '-t', 'overlay', '-o', 'owned-options', 'none', cwd], 'cwd': None})
  source['commands'].extend(copy.deepcopy(transition))
  native['commands'].extend(copy.deepcopy(transition))
  source['commands'].append({'argv': ['git', 'ls-remote', 'origin', 'HEAD'], 'cwd': cwd})
  native['commands'].extend([
    {'argv': ['git', 'rev-parse', '--abbrev-ref', 'HEAD'], 'cwd': cwd},
    {'argv': ['git', 'config', '--get', 'branch.dev.remote' if reset_overlay else 'branch.release-tizi.remote'], 'cwd': cwd},
    {'argv': ['git', 'remote', 'get-url', 'origin'], 'cwd': cwd},
    {'argv': ['git', 'ls-remote', 'origin', 'HEAD'], 'cwd': cwd},
  ])
  assert assert_equivalent(source, native) == 1
  native['commands'][-3]['argv'][-1] = 'branch.wrong.remote'
  with pytest.raises(AssertionError):
    assert_equivalent(source, native)


@pytest.mark.parametrize('change', ['branch', 'remote', 'cwd', 'missing', 'extra', 'reordered', 'implicit-heads', 'other-heads', 'write', 'snapshot'])
def test_remote_exception_rejects_any_other_change(change):
  source, native = remote_traces()
  if change == 'branch':
    native['commands'][1]['argv'][-1] = 'branch.other.remote'
  elif change == 'remote':
    native['commands'][2]['argv'][-1] = 'rust'
  elif change == 'cwd':
    native['commands'][0]['cwd'] = '<ROOT>/base'
  elif change == 'missing':
    native['commands'].pop(1)
  elif change == 'extra':
    native['commands'].insert(0, copy.deepcopy(native['commands'][0]))
  elif change == 'reordered':
    native['commands'][1:3] = native['commands'][1:3][::-1]
  elif change == 'implicit-heads':
    native['commands'][9]['argv'].pop()
  elif change == 'other-heads':
    native['commands'][9]['argv'][-1] = 'rust'
  elif change == 'write':
    native['commands'][1]['argv'] = ['git', 'config', 'branch.dev.remote', 'origin']
  else:
    native['snapshots'][0]['params']['UpdateAvailable'] = '30'
  with pytest.raises(AssertionError):
    assert_equivalent(source, native)


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
