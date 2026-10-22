from openpilot.selfdrive.modeld.jetlink.transition import Transition, ControlState, Mode, Outcome, required_process_failures

STOP = ControlState(True, False, False, False)
DRIVE = ControlState(False, True, True, True)


def test_optional_owner_failure_does_not_disable_native():
  assert required_process_failures({'jetlinkd'}, False) == set()
  assert required_process_failures({'jetlinkd', 'modeld'}, False) == {'modeld'}
  assert required_process_failures({'jetlinkd'}, True) == {'jetlinkd'}


def activate(state):
  return state.update(Mode.ACTIVE_REQUEST, True, True, STOP, Outcome.VALID)


def test_both_historical_external_values_activate_directly():
  for mode in (1, 2):
    state = Transition()
    decision = state.update(mode, True, True, STOP, Outcome.VALID)
    assert decision.source == 'jetlink' and decision.reset_required
    assert not state.update(mode, True, True, STOP, Outcome.VALID).reset_required


def test_ready_without_live_contract_stays_native_until_compatible():
  state = Transition()
  assert state.update(Mode.ACTIVE_REQUEST, True, False, STOP, Outcome.VALID).source == 'native'
  assert state.update(Mode.ACTIVE_REQUEST, True, True, STOP, Outcome.VALID).source == 'jetlink'


def test_switch_waits_for_stop_and_every_control_to_disengage():
  state = Transition()
  for controls in (DRIVE, ControlState(True, True, False, False),
                   ControlState(True, False, True, False), ControlState(True, False, False, True)):
    assert state.update(Mode.ACTIVE_REQUEST, True, True, controls, Outcome.VALID).source == 'native'
  assert activate(state).source == 'jetlink'


def test_single_late_frame_latches_loss_and_native_success_cannot_clear():
  state = Transition()
  assert activate(state).source == 'jetlink'
  lost = state.update(Mode.ACTIVE_REQUEST, False, True, DRIVE, Outcome.LOST)
  assert lost.loss_latched and lost.source == 'native'
  assert state.update(Mode.ACTIVE_REQUEST, True, True, DRIVE, Outcome.VALID).loss_latched
  assert state.update(Mode.OFF, True, True, DRIVE, Outcome.VALID).loss_latched


def test_reconnect_requires_manual_inactive_stop():
  state = Transition(); activate(state)
  state.update(Mode.ACTIVE_REQUEST, False, True, DRIVE, Outcome.LOST)
  assert state.update(Mode.ACTIVE_REQUEST, True, True, STOP, Outcome.VALID).loss_latched
  # The old 1 -> 2 edge is no longer an acknowledgement.
  assert state.update(2, True, True, STOP, Outcome.VALID).loss_latched
  cleared = state.update(Mode.OFF, False, False, STOP, Outcome.NONE)
  assert not cleared.loss_latched and cleared.source == 'native'
  assert state.update(Mode.ACTIVE_REQUEST, True, True, STOP, Outcome.VALID).reset_required


def test_owner_death_or_process_restart_does_not_auto_activate():
  state = Transition(previously_active=True)
  decision = state.update(Mode.ACTIVE_REQUEST, True, True, STOP, Outcome.VALID)
  assert decision.source == 'native' and decision.loss_latched
  fresh = Transition()
  assert fresh.update(Mode.ACTIVE_REQUEST, True, True, STOP, Outcome.VALID).source == 'jetlink'


def test_setting_change_or_validation_loss_while_active_latches():
  state = Transition(); activate(state)
  assert state.update(Mode.OFF, True, True, DRIVE, Outcome.VALID).loss_latched
  state = Transition(); activate(state)
  assert state.update(Mode.ACTIVE_REQUEST, True, False, DRIVE, Outcome.VALID).loss_latched


def test_off_and_unknown_values_never_select_external():
  for mode in (0, -1, 3, 2147483647):
    assert Transition().update(mode, True, True, STOP, Outcome.VALID).source == 'native'
