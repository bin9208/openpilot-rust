use super::{
    state::{float, Bus, State},
    Error,
};
use openpilot_cereal::car_capnp::{
    car_params::{NetworkLocation, TransmissionType},
    car_state::{self, GearShifter},
};
impl State {
    pub(super) fn pq(&mut self, ret: &mut car_state::Builder<'_>, now: u64) -> Result<(), Error> {
        let mut values = [0.; 4];
        for (index, key) in [
            "Radgeschw__VL_4_1",
            "Radgeschw__VR_4_1",
            "Radgeschw__HL_4_1",
            "Radgeschw__HR_4_1",
        ]
        .into_iter()
        .enumerate()
        {
            values[index] = float(
                self.signal(Bus::Pt, ("Bremse_3", key), now)?
                    * ((1. / 3.6) * self.config.wheel_factor),
            )?;
        }
        let mut wheels = ret.reborrow().init_wheel_speeds();
        wheels.set_fl(values[0]);
        wheels.set_fr(values[1]);
        wheels.set_rl(values[2]);
        wheels.set_rr(values[3]);
        let raw = float(
            self.signal(Bus::Pt, ("Bremse_1", "Geschwindigkeit_neu__Bremse_1_"), now)? * (1. / 3.6),
        )?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_standstill(raw == 0.);
        ret.set_steering_angle_deg(float(self.signed(
            Bus::Pt,
            ("Lenkhilfe_3", "LH3_BLW", "LH3_BLWSign"),
            now,
        )?)?);
        ret.set_steering_rate_deg(float(self.signed(
            Bus::Pt,
            (
                "Lenkwinkel_1",
                "Lenkradwinkel_Geschwindigkeit",
                "Lenkradwinkel_Geschwindigkeit_S",
            ),
            now,
        )?)?);
        let torque = float(self.signed(Bus::Pt, ("Lenkhilfe_3", "LH3_LM", "LH3_LMSign"), now)?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_pressed(f64::from(torque.abs()) > self.config.driver_allowance());
        ret.set_yaw_rate(float(
            self.signed(
                Bus::Pt,
                (
                    "Bremse_5",
                    "Giergeschwindigkeit",
                    "Vorzeichen_der_Giergeschwindigk",
                ),
                now,
            )? * (std::f64::consts::PI / 180.),
        )?);
        let value = self.signal(Bus::Pt, ("Lenkhilfe_2", "LH2_Sta_HCA"), now)?;
        let status = self.hca_status(value)?;
        self.hca_faults(ret, status.as_deref(), true);
        let gas = float(self.signal(Bus::Pt, ("Motor_3", "Fahrpedal_Rohsignal"), now)? / 100.)?;
        ret.set_gas(gas);
        ret.set_gas_pressed(gas > 0.);
        ret.set_brake(float(
            self.signal(Bus::Pt, ("Bremse_5", "Bremsdruck"), now)? / 250.,
        )?);
        ret.set_brake_pressed(self.signal(Bus::Pt, ("Motor_2", "Bremslichtschalter"), now)? != 0.);
        ret.set_parking_brake(self.signal(Bus::Pt, ("Kombi_1", "Bremsinfo"), now)? != 0.);
        match self.config.transmission {
            TransmissionType::Automatic => {
                let value = self.signal(
                    Bus::Pt,
                    ("Getriebe_1", "Waehlhebelposition__Getriebe_1_"),
                    now,
                )?;
                ret.set_gear_shifter(self.gear(value)?);
            }
            TransmissionType::Manual => {
                ret.set_clutch_pressed(
                    self.signal(Bus::Pt, ("Motor_1", "Kupplungsschalter"), now)? == 0.,
                );
                ret.set_gear_shifter(
                    if self.signal(Bus::Pt, ("Gate_Komf_1", "GK1_Rueckfahr"), now)? != 0. {
                        GearShifter::Reverse
                    } else {
                        GearShifter::Drive
                    },
                );
            }
            TransmissionType::Unknown | TransmissionType::Direct | TransmissionType::Cvt => {}
        }
        let mut door = false;
        for key in [
            "GK1_Fa_Tuerkont",
            "BSK_BT_geoeffnet",
            "BSK_HL_geoeffnet",
            "BSK_HR_geoeffnet",
            "BSK_HD_Hauptraste",
        ] {
            door |= self.signal(Bus::Pt, ("Gate_Komf_1", key), now)? != 0.;
        }
        ret.set_door_open(door);
        ret.set_seatbelt_unlatched(
            self.signal(Bus::Pt, ("Airbag_1", "Gurtschalter_Fahrer"), now)? == 0.,
        );
        if self.config.bsm {
            ret.set_left_blindspot(
                self.signal(Bus::External, ("SWA_1", "SWA_Infostufe_SWA_li"), now)? != 0.
                    || self.signal(Bus::External, ("SWA_1", "SWA_Warnung_SWA_li"), now)? != 0.,
            );
            ret.set_right_blindspot(
                self.signal(Bus::External, ("SWA_1", "SWA_Infostufe_SWA_re"), now)? != 0.
                    || self.signal(Bus::External, ("SWA_1", "SWA_Warnung_SWA_re"), now)? != 0.,
            );
        }
        self.extras.ldw_stock_values = Some(match self.config.network {
            NetworkLocation::FwdCamera => self.copied(Bus::Cam, "LDW_Status", now)?,
            NetworkLocation::Gateway => Default::default(),
        });
        self.extras.acc_type =
            Some(self.signal(Bus::External, ("ACC_System", "ACS_Typ_ACC"), now)?);
        let available = self.signal(Bus::Pt, ("Motor_5", "GRA_Hauptschalter"), now)? != 0.;
        let status = self.signal(Bus::Pt, ("Motor_2", "GRA_Status"), now)?;
        let fault = if self.config.pcm {
            matches!(
                self.signal(Bus::External, ("ACC_GRA_Anzeige", "ACA_StaACC"), now)?,
                6. | 7.
            )
        } else {
            status == 3.
        };
        ret.set_acc_faulted(fault);
        let speed = float(
            self.signal(Bus::External, ("ACC_GRA_Anzeige", "ACA_V_Wunsch"), now)? * (1. / 3.6),
        )?;
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_available(available);
        cruise.set_enabled(status == 1. || status == 2.);
        cruise.set_speed(if speed > 70. { 0. } else { speed });
        let left = self.signal(Bus::Pt, ("Gate_Komf_1", "GK1_Blinker_li"), now)? != 0.;
        let right = self.signal(Bus::Pt, ("Gate_Komf_1", "GK1_Blinker_re"), now)? != 0.;
        self.blinkers(ret, 300, [left, right]);
        self.buttons(ret, now, false)?;
        self.extras.gra_stock_values = Some(self.copied(Bus::Pt, "GRA_Neu", now)?);
        ret.set_esp_disabled(self.signal(Bus::Pt, ("Bremse_1", "ESP_Passiv_getastet"), now)? != 0.);
        Ok(())
    }
}
