use crate::{health::CanHealth, state::Snapshot};
use capnp::{dynamic_value, introspect::TypeVariant};
use openpilot_cereal::log_capnp::{event, panda_state};

pub(crate) fn raw_enum(
    value: dynamic_value::Builder<'_>,
    name: &str,
    ordinal: u16,
) -> capnp::Result<()> {
    let dynamic_value::Builder::Struct(mut builder) = value else {
        return Err(capnp::Error::failed("enum field requires a struct".into()));
    };
    let field = builder.get_schema().get_field_by_name(name)?;
    let TypeVariant::Enum(schema) = field.get_type().which() else {
        return Err(capnp::Error::failed(format!("{name} is not an enum")));
    };
    builder.set(
        field,
        dynamic_value::Reader::Enum(dynamic_value::Enum::new(ordinal, schema.into())),
    )
}

fn can_state(
    mut builder: panda_state::panda_can_state::Builder<'_>,
    state: CanHealth,
) -> capnp::Result<()> {
    builder.set_bus_off(state.bus_off != 0);
    builder.set_bus_off_cnt(state.bus_off_count);
    builder.set_error_warning(state.error_warning != 0);
    builder.set_error_passive(state.error_passive != 0);
    raw_enum(
        builder.reborrow().into(),
        "lastError",
        u16::from(state.last_error),
    )?;
    raw_enum(
        builder.reborrow().into(),
        "lastStoredError",
        u16::from(state.last_stored_error),
    )?;
    raw_enum(
        builder.reborrow().into(),
        "lastDataError",
        u16::from(state.last_data_error),
    )?;
    raw_enum(
        builder.reborrow().into(),
        "lastDataStoredError",
        u16::from(state.last_data_stored_error),
    )?;
    builder.set_receive_error_cnt(state.receive_error_count);
    builder.set_transmit_error_cnt(state.transmit_error_count);
    builder.set_total_error_cnt(state.total_errors);
    builder.set_total_tx_lost_cnt(state.total_tx_lost);
    builder.set_total_rx_lost_cnt(state.total_rx_lost);
    builder.set_total_tx_cnt(state.total_tx);
    builder.set_total_rx_cnt(state.total_rx);
    builder.set_total_fwd_cnt(state.total_forwarded);
    builder.set_can_speed(state.can_speed);
    builder.set_can_data_speed(state.can_data_speed);
    builder.set_canfd_enabled(state.canfd_enabled != 0);
    builder.set_brs_enabled(state.brs_enabled != 0);
    builder.set_canfd_non_iso(state.canfd_non_iso != 0);
    builder.set_irq0_call_rate(state.irq0_rate);
    builder.set_irq1_call_rate(state.irq1_rate);
    builder.set_irq2_call_rate(state.irq2_rate);
    builder.set_can_core_reset_cnt(state.core_reset_count);
    Ok(())
}

pub fn encode(states: &[Snapshot], valid: bool, monotonic_ns: u64) -> capnp::Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(monotonic_ns);
    event.set_valid(valid);
    let count = u32::try_from(states.len())
        .map_err(|_| capnp::Error::failed("too many Panda states".into()))?;
    let mut list = event.init_panda_states(count);
    for (index, snapshot) in (0..count).zip(states) {
        let mut builder = list.reborrow().get(index);
        let health = snapshot.health;
        builder.set_voltage(health.voltage);
        builder.set_current(health.current);
        builder.set_uptime(health.uptime);
        builder.set_safety_tx_blocked(health.safety_tx_blocked);
        builder.set_safety_rx_invalid(health.safety_rx_invalid);
        builder.set_ignition_line(health.ignition_line != 0);
        builder.set_ignition_can(health.ignition_can != 0);
        builder.set_controls_allowed(health.controls_allowed != 0);
        builder.set_tx_buffer_overflow(health.tx_overflow);
        builder.set_rx_buffer_overflow(health.rx_overflow);
        raw_enum(
            builder.reborrow().into(),
            "pandaType",
            u16::from(snapshot.identity.hardware_type),
        )?;
        raw_enum(
            builder.reborrow().into(),
            "safetyModel",
            u16::from(health.safety_model),
        )?;
        builder.set_safety_param(health.safety_param);
        raw_enum(
            builder.reborrow().into(),
            "faultStatus",
            u16::from(health.fault_status),
        )?;
        builder.set_power_save_enabled(health.power_save != 0);
        builder.set_heartbeat_lost(health.heartbeat_lost != 0);
        builder.set_alternative_experience(i16::from_le_bytes(
            health.alternative_experience.to_le_bytes(),
        ));
        raw_enum(
            builder.reborrow().into(),
            "harnessStatus",
            u16::from(health.harness_status),
        )?;
        builder.set_interrupt_load(health.interrupt_load);
        builder.set_fan_power(health.fan_power);
        builder.set_fan_stall_count(health.fan_stall_count);
        builder.set_safety_rx_checks_invalid(health.safety_rx_checks_invalid != 0);
        builder.set_spi_checksum_error_count(health.spi_checksum_errors);
        builder.set_sbu1_voltage(f32::from(health.sbu1_mv) / 1000.0);
        builder.set_sbu2_voltage(f32::from(health.sbu2_mv) / 1000.0);
        can_state(builder.reborrow().init_can_state0(), snapshot.can[0])?;
        can_state(builder.reborrow().init_can_state1(), snapshot.can[1])?;
        can_state(builder.reborrow().init_can_state2(), snapshot.can[2])?;
        let mut faults = builder.init_faults(health.faults.count_ones());
        let mut fault_index = 0;
        for ordinal in 0..=u16::from(panda_state::FaultType::HeartbeatLoopWatchdog) {
            if health.faults & (1 << ordinal) != 0 {
                let fault = panda_state::FaultType::try_from(ordinal)
                    .map_err(|error| capnp::Error::failed(error.to_string()))?;
                faults.set(fault_index, fault);
                fault_index += 1;
            }
        }
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
