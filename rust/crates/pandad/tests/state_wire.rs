use openpilot_cereal::log_capnp::{event, panda_state::FaultType};
use openpilot_pandad::{
    health::{CanHealth, Health},
    state::{Identity, Snapshot},
    state_wire,
};

#[test]
fn state_wire_preserves_unknown_enum_ordinals_and_source_fault_list_allocation() {
    let snapshot = Snapshot {
        identity: Identity {
            hardware_type: 255,
            serial: vec![],
        },
        health: Health {
            safety_model: 250,
            fault_status: 240,
            harness_status: 230,
            faults: (1 << 31) | 4,
            alternative_experience: 65535,
            sbu1_mv: 1234,
            ..Health::default()
        },
        can: [CanHealth {
            last_error: 222,
            ..CanHealth::default()
        }; 3],
    };
    let data = state_wire::encode(&[snapshot], false, 1234).unwrap();
    let mut bytes = data.as_slice();
    let message = capnp::serialize::read_message_from_flat_slice(
        &mut bytes,
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    let root = message.get_root::<event::Reader>().unwrap();
    assert_eq!(root.get_log_mono_time(), 1234);
    assert!(!root.get_valid());
    let event::PandaStates(states) = root.which().unwrap() else {
        panic!("wrong event")
    };
    let state = states.unwrap().get(0);
    assert_eq!(state.get_panda_type(), Err(capnp::NotInSchema(255)));
    assert_eq!(state.get_safety_model(), Err(capnp::NotInSchema(250)));
    assert_eq!(state.get_fault_status(), Err(capnp::NotInSchema(240)));
    assert_eq!(state.get_harness_status(), Err(capnp::NotInSchema(230)));
    assert_eq!(
        state.get_can_state0().unwrap().get_last_error(),
        Err(capnp::NotInSchema(222))
    );
    assert_eq!(state.get_alternative_experience(), -1);
    assert_eq!(state.get_sbu1_voltage(), 1.234_f32);
    let faults = state.get_faults().unwrap();
    assert_eq!(faults.len(), 2);
    assert_eq!(faults.get(0), Ok(FaultType::InterruptRateCan1));
    assert_eq!(faults.get(1), Ok(FaultType::RelayMalfunction));
}
