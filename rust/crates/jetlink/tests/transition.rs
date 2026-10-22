use openpilot_jetlink::transition::{ControlState, Mode, Outcome, Phase, Source, Transition};

const STOP: ControlState = ControlState {
    standstill: true,
    cruise_enabled: false,
    lateral_active: false,
    enabled: false,
};

#[test]
fn persisted_external_values_select_directly_and_unknown_values_stay_off() {
    for value in [1, 2] {
        let mut state = Transition::new(false);
        let decision = state.update(Mode::from_setting(value), true, true, STOP, Outcome::Valid);
        assert_eq!(decision.source, Source::Jetlink);
        assert!(decision.reset_required);
        assert!(
            !state
                .update(Mode::from_setting(value), true, true, STOP, Outcome::Valid)
                .reset_required
        );
    }
    for value in [0, -1, 3, i32::MAX] {
        let decision = Transition::new(false).update(
            Mode::from_setting(value),
            true,
            true,
            STOP,
            Outcome::Valid,
        );
        assert_eq!(decision.source, Source::Native);
        assert_eq!(decision.phase, Phase::Off);
    }
}

#[test]
fn selection_waits_for_live_contract_and_inactive_stop_without_another_mode_edge() {
    let mode = Mode::from_setting(1);
    let mut state = Transition::new(false);
    for controls in [
        ControlState {
            standstill: false,
            ..STOP
        },
        ControlState {
            cruise_enabled: true,
            ..STOP
        },
        ControlState {
            lateral_active: true,
            ..STOP
        },
        ControlState {
            enabled: true,
            ..STOP
        },
    ] {
        assert_eq!(
            state
                .update(mode, true, true, controls, Outcome::Valid)
                .source,
            Source::Native
        );
    }
    assert_eq!(
        state.update(mode, false, true, STOP, Outcome::None).source,
        Source::Native
    );
    assert_eq!(
        state.update(mode, true, false, STOP, Outcome::Valid).source,
        Source::Native
    );
    assert_eq!(
        state.update(mode, true, true, STOP, Outcome::Lost).source,
        Source::Native
    );
    assert_eq!(
        state.update(mode, true, true, STOP, Outcome::Valid).source,
        Source::Jetlink
    );
}

#[test]
fn loss_and_restart_require_off_acknowledgement_at_inactive_stop() {
    let mode = Mode::from_setting(1);
    for restarted in [false, true] {
        let mut state = Transition::new(restarted);
        if !restarted {
            assert_eq!(
                state.update(mode, true, true, STOP, Outcome::Valid).source,
                Source::Jetlink
            );
            assert!(
                state
                    .update(mode, false, true, STOP, Outcome::Lost)
                    .loss_latched
            );
        }
        // Reconnection, stopping, or changing between old external values cannot acknowledge loss.
        for value in [1, 2] {
            assert!(
                state
                    .update(Mode::from_setting(value), true, true, STOP, Outcome::Valid)
                    .loss_latched
            );
        }
        assert!(
            state
                .update(
                    Mode::Off,
                    true,
                    true,
                    ControlState {
                        lateral_active: true,
                        ..STOP
                    },
                    Outcome::None
                )
                .loss_latched
        );
        let cleared = state.update(Mode::Off, false, false, STOP, Outcome::None);
        assert!(!cleared.loss_latched);
        assert_eq!(cleared.source, Source::Native);
        assert!(
            state
                .update(mode, true, true, STOP, Outcome::Valid)
                .reset_required
        );
    }
}

#[test]
fn active_contract_loss_and_deselection_still_latch() {
    for (mode, ready, compatible) in [
        (Mode::Off, true, true),
        (Mode::from_setting(1), false, true),
        (Mode::from_setting(1), true, false),
    ] {
        let mut state = Transition::new(false);
        state.update(Mode::from_setting(1), true, true, STOP, Outcome::Valid);
        let decision = state.update(
            mode,
            ready,
            compatible,
            ControlState {
                standstill: false,
                ..STOP
            },
            Outcome::Valid,
        );
        assert!(decision.loss_latched);
        assert_eq!(decision.source, Source::Native);
    }
}
