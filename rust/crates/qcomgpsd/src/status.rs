use openpilot_cereal::log_capnp::qcom_gnss::measurement_status;
pub enum Source {
    Gps(u8),
    Glonass(u8),
    Dr { multipath: u8, direction: u8 },
}
pub fn fill(mut target: measurement_status::Builder<'_>, bits: u32, source: Source) {
    target.set_sub_millisecond_is_valid(bits & (1 << 0) != 0);
    target.set_sub_bit_time_is_known(bits & (1 << 1) != 0);
    target.set_satellite_time_is_known(bits & (1 << 2) != 0);
    target.set_bit_edge_confirmed_from_signal(bits & (1 << 3) != 0);
    target.set_measured_velocity(bits & (1 << 4) != 0);
    target.set_fine_or_coarse_velocity(bits & (1 << 5) != 0);
    target.set_lock_point_valid(bits & (1 << 6) != 0);
    target.set_lock_point_positive(bits & (1 << 7) != 0);
    target.set_last_update_from_difference(bits & (1 << 9) != 0);
    target.set_last_update_from_velocity_difference(bits & (1 << 10) != 0);
    target.set_strong_indication_of_cross_corelation(bits & (1 << 11) != 0);
    target.set_tentative_measurement(bits & (1 << 12) != 0);
    target.set_measurement_not_usable(bits & (1 << 13) != 0);
    target.set_sir_check_is_needed(bits & (1 << 14) != 0);
    target.set_probation_mode(bits & (1 << 15) != 0);
    target.set_multipath_indicator(bits & (1 << 24) != 0);
    target.set_imd_jamming_indicator(bits & (1 << 25) != 0);
    target.set_lte_b13_tx_jamming_indicator(bits & (1 << 26) != 0);
    target.set_fresh_measurement_indicator(bits & (1 << 27) != 0);
    let (multipath, direction) = match source {
        Source::Gps(misc) => {
            target.set_gps_round_robin_rx_diversity(bits & (1 << 18) != 0);
            target.set_gps_rx_diversity(bits & (1 << 19) != 0);
            target.set_gps_low_bandwidth_rx_diversity_combined(bits & (1 << 20) != 0);
            target.set_gps_high_bandwidth_nu4(bits & (1 << 21) != 0);
            target.set_gps_high_bandwidth_nu8(bits & (1 << 22) != 0);
            target.set_gps_high_bandwidth_uniform(bits & (1 << 23) != 0);
            (misc & 1 != 0, misc & 2 != 0)
        }
        Source::Glonass(misc) => {
            target.set_glonass_meander_bit_edge_valid(bits & (1 << 16) != 0);
            target.set_glonass_time_mark_valid(bits & (1 << 17) != 0);
            (misc & 1 != 0, misc & 2 != 0)
        }
        Source::Dr {
            multipath,
            direction,
        } => (multipath != 0, direction != 0),
    };
    target.set_multipath_estimate_is_valid(multipath);
    target.set_direction_is_valid(direction);
}
