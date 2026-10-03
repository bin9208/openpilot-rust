use super::{
    state::{float, snapshot, State},
    Error,
};
use crate::{core::Message, state_helpers};
use num_traits::ToPrimitive;
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::{
    car_params::TransmissionType,
    car_state::{self, button_event::Type as Button, GearShifter},
};
impl State {
    pub fn update(&mut self, packets: &[Packet], now: u64) -> Result<Message, Error> {
        self.pt.update(packets)?;
        self.camera.update(packets)?;
        self.drain_logs();
        let mut message = Message::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        ret.set_vehicle_sensors_invalid(
            self.signal("SteeringPinion_Data", "StePinCompAnEst_D_Qf", now)? != 3.,
        );
        let raw = float(self.signal("BrakeSysFeatures", "Veh_V_ActlBrk", now)? * (1. / 3.6))?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_yaw_rate(float(self.signal("Yaw_Data_FD1", "VehYaw_W_Actl", now)?)?);
        ret.set_standstill(self.signal("DesiredTorqBrk", "VehStop_D_Stat", now)? == 1.);
        let gas = float(self.signal("EngVehicleSpThrottle", "ApedPos_Pc_ActlArb", now)? / 100.)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(f64::from(gas) > 1e-6);
        ret.set_brake(float(
            self.signal("BrakeSnData_4", "BrkTot_Tq_Actl", now)? / 32756.,
        )?);
        ret.set_brake_pressed(self.signal("EngBrakeData", "BpedDrvAppl_D_Actl", now)? == 2.);
        let parking = self.signal("DesiredTorqBrk", "PrkBrkStatus", now)?;
        ret.set_parking_brake(parking == 1. || parking == 2.);
        ret.set_steering_angle_deg(float(self.signal(
            "SteeringPinion_Data",
            "StePinComp_An_Est",
            now,
        )?)?);
        let torque = float(self.signal("EPAS_INFO", "SteeringColumnTorque", now)?)?;
        ret.set_steering_torque(torque);
        self.extras.steering_pressed_cnt = if torque.abs() > 1. {
            self.extras.steering_pressed_cnt.saturating_add(1).min(6)
        } else {
            0
        };
        ret.set_steering_pressed(self.extras.steering_pressed_cnt > 5);
        let failure = self.signal("EPAS_INFO", "EPAS_Failure", now)?;
        let mut temporary = failure == 1.;
        ret.set_steer_fault_permanent(failure == 2. || failure == 3.);
        ret.set_esp_disabled(self.signal("Cluster_Info1_FD1", "DrvSlipCtlMde_D_Rq", now)? != 0.);
        if self.config.canfd {
            let status = self.signal("Lane_Assist_Data3_FD1", "LatCtlSte_D_Stat", now)?;
            temporary |= !(status == 1. || status == 2. || status == 3.);
        }
        ret.set_steer_fault_temporary(temporary);
        let metric =
            !self.config.canfd && self.signal("INSTRUMENT_PANEL", "METRIC_UNITS", now)? == 1.;
        let cruise_speed = float(
            self.signal("EngBrakeData", "Veh_V_DsplyCcSet", now)?
                * if metric {
                    1. / 3.6
                } else {
                    1.609344 * (1. / 3.6)
                },
        )?;
        let status = self.signal("EngBrakeData", "CcStat_D_Actl", now)?;
        let nonadaptive = self.signal("Cluster_Info1_FD1", "AccEnbl_B_RqDrv", now)? == 0.;
        let standstill = self.signal("EngBrakeData", "AccStopMde_D_Rq", now)? == 3.;
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_speed(cruise_speed);
        cruise.set_enabled(status == 4. || status == 5.);
        cruise.set_available(status == 3. || status == 4. || status == 5.);
        cruise.set_non_adaptive(nonadaptive);
        cruise.set_standstill(standstill);
        let mut fault = status == 1. || status == 2.;
        if !self.config.longitudinal {
            fault = fault || self.camera_signal("ACCDATA", "CmbbDeny_B_Actl", now)? == 1.;
        }
        ret.set_acc_faulted(fault);
        match self.config.transmission {
            TransmissionType::Automatic => {
                let gear = self
                    .signal("PowertrainData_10", "TrnRng_D_Rq", now)?
                    .to_i64()
                    .ok_or(Error::Numeric)?;
                let address = self.pt.dbc.message("PowertrainData_10")?.address;
                ret.set_gear_shifter(state_helpers::parse_gear(
                    self.defs
                        .get(&address)
                        .and_then(|d| d.get("TrnRng_D_Rq"))
                        .and_then(|d| d.get(&gear))
                        .map(String::as_str),
                ));
            }
            TransmissionType::Manual => {
                ret.set_clutch_pressed(
                    self.signal("Engine_Clutch_Data", "CluPdlPos_Pc_Meas", now)? > 0.,
                );
                ret.set_gear_shifter(
                    if self.signal("BCM_Lamp_Stat_FD1", "RvrseLghtOn_B_Stat", now)? != 0. {
                        GearShifter::Reverse
                    } else {
                        GearShifter::Drive
                    },
                );
            }
            TransmissionType::Unknown | TransmissionType::Direct | TransmissionType::Cvt => {}
        }
        ret.set_engine_rpm(float(self.signal(
            "EngVehicleSpThrottle",
            "EngAout_N_Actl",
            now,
        )?)?);
        ret.set_stock_fcw(self.camera_signal("ACCDATA_3", "FcwVisblWarn_B_Rq", now)? != 0.);
        ret.set_stock_aeb(self.camera_signal("ACCDATA_2", "CmbbBrkDecel_B_Rq", now)? != 0.);
        let turn = self.signal("Steering_Data_FD1", "TurnLghtSwtch_D_Stat", now)?;
        ret.set_left_blinker(turn == 1.);
        ret.set_right_blinker(turn == 2.);
        let toggle = self.signal("Steering_Data_FD1", "TjaButtnOnOffPress", now)? != 0.;
        ret.set_generic_toggle(toggle);
        let previous_distance = self.extras.distance_button;
        let previous_lc = self.extras.lc_button;
        self.extras.distance_button =
            self.signal("Steering_Data_FD1", "AccButtnGapTogglePress", now)?;
        self.extras.lc_button = toggle;
        let mut door = false;
        for key in [
            "DrStatDrv_B_Actl",
            "DrStatPsngr_B_Actl",
            "DrStatRl_B_Actl",
            "DrStatRr_B_Actl",
        ] {
            door |= self.signal("BodyInfo_3_FD1", key, now)? != 0.;
        }
        ret.set_door_open(door);
        ret.set_seatbelt_unlatched(
            self.signal("RCMStatusMessage2_FD1", "FirstRowBuckleDriver", now)? == 2.,
        );
        if self.config.blindspots {
            let parser = if self.config.canfd {
                &mut self.camera
            } else {
                &mut self.pt
            };
            ret.set_left_blindspot(
                parser.signal_lazy("Side_Detect_L_Stat", "SodDetctLeft_D_Stat", now)? != 0.,
            );
            ret.set_right_blindspot(
                parser.signal_lazy("Side_Detect_R_Stat", "SodDetctRight_D_Stat", now)? != 0.,
            );
        }
        self.extras.buttons_stock_values = Some(snapshot(&mut self.pt, "Steering_Data_FD1", now)?);
        self.extras.acc_tja_status_stock_values =
            Some(snapshot(&mut self.camera, "ACCDATA_3", now)?);
        self.extras.lkas_status_stock_values = Some(snapshot(&mut self.camera, "IPMA_Data", now)?);
        let mut events = Vec::new();
        for (current, previous, kind) in [
            (
                self.extras.distance_button,
                previous_distance,
                Button::GapAdjustCruise,
            ),
            (f64::from(toggle), f64::from(previous_lc), Button::Lkas),
        ] {
            if current != previous {
                if previous != 0. {
                    events.push((
                        if previous == 1. {
                            kind
                        } else {
                            Button::Unknown
                        },
                        false,
                    ));
                }
                if current != 0. {
                    events.push((if current == 1. { kind } else { Button::Unknown }, true));
                }
            }
        }
        let mut buttons = ret
            .reborrow()
            .init_button_events(u32::try_from(events.len()).map_err(|_| Error::Numeric)?);
        for (index, (kind, pressed)) in events.into_iter().enumerate() {
            let mut button = buttons
                .reborrow()
                .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
            button.set_type(kind);
            button.set_pressed(pressed);
        }
        ret.set_can_valid(self.pt.can_valid() && self.camera.can_valid());
        ret.set_can_timeout(self.pt.bus_timeout() || self.camera.bus_timeout());
        if ret.reborrow_as_reader().get_v_ego_cluster() == 0. && !self.cluster_seen {
            ret.set_v_ego_cluster(ret.reborrow_as_reader().get_v_ego());
        } else {
            self.cluster_seen = true;
        }
        ret.reborrow()
            .get_cruise_state()?
            .set_speed_cluster(cruise_speed);
        ret.set_button_enable(state_helpers::button_enable(
            self.config.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?);
        self.out.set_root(ret.into_reader())?;
        self.drain_logs();
        Ok(message)
    }
}
