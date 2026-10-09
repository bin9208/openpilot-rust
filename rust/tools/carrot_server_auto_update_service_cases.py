from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  mode: str = 'reboot'
  reboot_mode: str = 'park'
  steps: tuple[dict, ...] = ()
  initial: dict = field(default_factory=dict)
  dirty: bool = False
  blocked: bool = False
  old: str = 'base'


READY = {'valid': True, 'car_valid': True, 'gear': 'GearShifter.PARK'}
CASES = (
  Case('pending-cancel-off', steps=({'mode': 'off'},)),
  Case('park-requires-live-disengaged-car', steps=({'valid': True}, READY | {'engaged': True}, READY)),
  Case('disengaged-exact-one-second', reboot_mode='disengaged', steps=({'now': 0., 'valid': True}, {'now': .999, 'valid': True}, {'now': 1., 'valid': True})),
  Case('disengaged-reset-and-device-offroad', reboot_mode='disengaged', steps=(
    {'now': 0., 'valid': True}, {'now': .8, 'valid': True}, {'now': 1.},
    {'now': 2., 'device_valid': True, 'started': False}, {'now': 3., 'device_valid': True, 'started': False},
  )),
  Case('changed-mode-resets-condition', steps=(
    {'now': 0., 'valid': True}, {'now': 2., 'mode': 'disengaged', 'valid': True},
    {'now': 2.999, 'mode': 'disengaged', 'valid': True}, {'now': 3., 'mode': 'disengaged', 'valid': True},
  )),
  Case('duplicate-request-blocked', initial={'status': 'reboot_requested', 'reboot_requested_head': 'head'}, steps=(READY,), old='target'),
  Case('pending-write-fails', blocked=True, steps=(READY,)),
  Case('clear-ref-error-preserves-receipt', mode='clear', initial={
    'status': 'error', 'error_code': 'pull_failed', 'error': "couldn't find remote ref owned", 'reboot_requested_head': 'receipt',
  }),
  Case('clear-multiple-ref-error', mode='clear', initial={
    'status': 'waiting', 'error_code': 'pull_failed', 'error': 'Cannot fast-forward to multiple branches',
  }),
  Case('clear-index-only-with-clean-head', mode='clear', initial={'status': 'error', 'error_code': 'reset_failed', 'error': 'index.lock File exists'}),
  Case('dirty-index-error-kept', mode='clear', initial={'status': 'error', 'error_code': 'git_busy', 'error': 'index.lock File exists'}, dirty=True),
  Case('unrelated-error-kept', mode='clear', initial={'status': 'error', 'error_code': 'pull_failed', 'error': 'owned unrelated failure'}),
  Case('notify-empty-old', mode='notify', old='empty'),
  Case('notify-unchanged-head', mode='notify', old='target'),
  Case('notify-real-commits-stats-and-unicode', mode='notify'),
)
