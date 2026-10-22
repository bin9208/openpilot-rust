use super::{
    state::{float, Bus, State},
    Error, STOCK_HCA_PRESENT,
};
use openpilot_cereal::car_capnp::{
    car_params::{NetworkLocation, TransmissionType},
    car_state::{self, GearShifter},
};
impl State {
    pub(super) fn mqb(&mut self, ret: &mut car_state::Builder<'_>, now: u64) -> Result<(), Error> {
        match self.config.transmission {
            TransmissionType::Direct => {
                let value = self.signal(Bus::Pt, ("Motor_EV_01", "MO_Waehlpos"), now)?;
                ret.set_gear_shifter(self.gear(value)?);
            }
            TransmissionType::Manual => {
                ret.set_clutch_pressed(
                    self.signal(Bus::Pt, ("Motor_14", "MO_Kuppl_schalter"), now)? == 0.,
                );
                ret.set_gear_shifter(
                    if self.signal(Bus::Pt, ("Gateway_72", "BCM1_Rueckfahrlicht_Schalter"), now)?
                        != 0.
                    {
                        GearShifter::Reverse
                    } else {
                        GearShifter::Drive
                    },
                );
            }
            TransmissionType::Automatic | TransmissionType::Unknown | TransmissionType::Cvt => {
                let value = self.signal(Bus::Pt, ("Gateway_73", "GE_Fahrstufe"), now)?;
                ret.set_gear_shifter(self.gear(value)?);
            }
        }
        self.extras.upscale_lead_car_signal =
            self.signal(Bus::Pt, ("Kombi_03", "KBI_Variante"), now)? != 0.;
        let mut values = [0.; 4];
        for (index, key) in [
            "ESP_VL_Radgeschw_02",
            "ESP_VR_Radgeschw_02",
            "ESP_HL_Radgeschw_02",
            "ESP_HR_Radgeschw_02",
        ]
        .into_iter()
        .enumerate()
        {
            values[index] = float(
                self.signal(Bus::Pt, ("ESP_19", key), now)?
                    * ((1. / 3.6) * self.config.wheel_factor),
            )?;
        }
        let mut wheels = ret.reborrow().init_wheel_speeds();
        wheels.set_fl(values[0]);
        wheels.set_fr(values[1]);
        wheels.set_rl(values[2]);
        wheels.set_rr(values[3]);
        ret.set_yaw_rate(float(
            self.signed(Bus::Pt, ("ESP_02", "ESP_Gierrate", "ESP_VZ_Gierrate"), now)?
                * (std::f64::consts::PI / 180.),
        )?);
        let hca = self.signal(Bus::Pt, ("LH_EPS_03", "EPS_HCA_Status"), now)?;
        let hca_status = self.hca_status(hca)?;
        if self.config.flags & STOCK_HCA_PRESENT != 0 {
            ret.set_car_faulted_non_critical(
                self.signal(Bus::Cam, ("HCA_01", "EA_Ruckfreigabe"), now)? != 0.
                    || self.signal(Bus::Cam, ("HCA_01", "EA_ACC_Sollstatus"), now)? > 0.,
            );
        }
        ret.set_gas(float(
            self.signal(Bus::Pt, ("Motor_20", "MO_Fahrpedalrohwert_01"), now)? / 100.,
        )?);
        ret.set_brake(float(
            self.signal(Bus::Pt, ("ESP_05", "ESP_Bremsdruck"), now)? / 250.,
        )?);
        let pedal = self.signal(Bus::Pt, ("Motor_14", "MO_Fahrer_bremst"), now)? != 0.;
        let pressure = self.signal(Bus::Pt, ("ESP_05", "ESP_Fahrer_bremst"), now)? != 0.;
        ret.set_brake_pressed(pedal || pressure);
        ret.set_parking_brake(self.signal(Bus::Pt, ("Kombi_01", "KBI_Handbremse"), now)? != 0.);
        let mut door = false;
        for key in [
            "ZV_FT_offen",
            "ZV_BT_offen",
            "ZV_HFS_offen",
            "ZV_HBFS_offen",
            "ZV_HD_offen",
        ] {
            door |= self.signal(Bus::Pt, ("Gateway_72", key), now)? != 0.;
        }
        ret.set_door_open(door);
        if self.config.bsm {
            ret.set_left_blindspot(
                self.signal(Bus::External, ("SWA_01", "SWA_Infostufe_SWA_li"), now)? != 0.
                    || self.signal(Bus::External, ("SWA_01", "SWA_Warnung_SWA_li"), now)? != 0.,
            );
            ret.set_right_blindspot(
                self.signal(Bus::External, ("SWA_01", "SWA_Infostufe_SWA_re"), now)? != 0.
                    || self.signal(Bus::External, ("SWA_01", "SWA_Warnung_SWA_re"), now)? != 0.,
            );
        }
        ret.set_stock_fcw(self.signal(Bus::External, ("ACC_10", "AWV2_Freigabe"), now)? != 0.);
        ret.set_stock_aeb(
            self.signal(Bus::External, ("ACC_10", "ANB_Teilbremsung_Freigabe"), now)? != 0.
                || self.signal(Bus::External, ("ACC_10", "ANB_Zielbremsung_Freigabe"), now)? != 0.,
        );
        self.extras.acc_type = Some(self.signal(Bus::External, ("ACC_06", "ACC_Typ"), now)?);
        self.extras.esp_hold_confirmation =
            self.signal(Bus::Pt, ("ESP_21", "ESP_Haltebestaetigung"), now)? != 0.;
        let acc_limiter =
            self.signal(Bus::External, ("ACC_02", "ACC_Gesetzte_Zeitluecke"), now)? == 0.;
        let speed_limiter = self.signal(Bus::Pt, ("TSK_06", "TSK_Limiter_ausgewaehlt"), now)? != 0.;
        let status = self.signal(Bus::Pt, ("TSK_06", "TSK_Status"), now)?;
        let speed = if self.config.pcm {
            float(self.signal(Bus::External, ("ACC_02", "ACC_Wunschgeschw_02"), now)? * (1. / 3.6))?
        } else {
            0.
        };
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_available(matches!(status, 2. | 3. | 4. | 5.));
        cruise.set_enabled(matches!(status, 3. | 4. | 5.));
        cruise.set_speed(speed);
        ret.set_acc_faulted(matches!(status, 6. | 7.));
        ret.set_left_blinker(
            self.signal(Bus::Pt, ("Blinkmodi_02", "Comfort_Signal_Left"), now)? != 0.,
        );
        ret.set_right_blinker(
            self.signal(Bus::Pt, ("Blinkmodi_02", "Comfort_Signal_Right"), now)? != 0.,
        );
        // NumPy receives the four stored cereal Float32 wheel values as Float64.
        let raw = float(values.into_iter().map(f64::from).sum::<f64>() / 4.)?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_steering_angle_deg(float(self.signed(
            Bus::Pt,
            ("LWI_01", "LWI_Lenkradwinkel", "LWI_VZ_Lenkradwinkel"),
            now,
        )?)?);
        ret.set_steering_rate_deg(float(self.signed(
            Bus::Pt,
            ("LWI_01", "LWI_Lenkradw_Geschw", "LWI_VZ_Lenkradw_Geschw"),
            now,
        )?)?);
        let torque = float(self.signed(
            Bus::Pt,
            ("LH_EPS_03", "EPS_Lenkmoment", "EPS_VZ_Lenkmoment"),
            now,
        )?)?;
        ret.set_steering_torque(torque);
        ret.set_steering_pressed(f64::from(torque.abs()) > self.config.driver_allowance());
        self.hca_faults(ret, hca_status.as_deref(), true);
        ret.set_gas_pressed(ret.reborrow_as_reader().get_gas() > 0.);
        ret.set_esp_active(self.signal(Bus::Pt, ("ESP_21", "ESP_Eingriff"), now)? != 0.);
        ret.set_esp_disabled(self.signal(Bus::Pt, ("ESP_21", "ESP_Tastung_passiv"), now)? != 0.);
        ret.set_seatbelt_unlatched(
            self.signal(Bus::Pt, ("Airbag_02", "AB_Gurtschloss_FA"), now)? != 3.,
        );
        ret.set_standstill(raw == 0.);
        let mut cruise = ret.reborrow().get_cruise_state()?;
        cruise.set_standstill(self.config.pcm && self.extras.esp_hold_confirmation);
        cruise.set_non_adaptive(acc_limiter || speed_limiter);
        if cruise.reborrow_as_reader().get_speed() > 90. {
            cruise.set_speed(0.);
        }
        self.extras.eps_stock_values =
            super::state::Stock::Values(self.copied(Bus::Pt, "LH_EPS_03", now)?);
        self.extras.ldw_stock_values = Some(match self.config.network {
            NetworkLocation::FwdCamera => self.copied(Bus::Cam, "LDW_02", now)?,
            NetworkLocation::Gateway => Default::default(),
        });
        self.extras.gra_stock_values = Some(self.copied(Bus::Pt, "GRA_ACC_01", now)?);
        self.buttons(ret, now, false)?;
        Ok(())
    }
}
