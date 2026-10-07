use super::{
    state::{float, Bus, State},
    Error, STOCK_HCA_PRESENT,
};
use openpilot_cereal::car_capnp::{
    car_params::TransmissionType,
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
        self.signal(Bus::Pt, ("LH_EPS_03", "EPS_HCA_Status"), now)?;
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
        self.signal(Bus::External, ("ACC_02", "ACC_Gesetzte_Zeitluecke"), now)?;
        self.signal(Bus::Pt, ("TSK_06", "TSK_Limiter_ausgewaehlt"), now)?;
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
        // The original module reaches np.mean here without importing numpy.
        Err(Error::InheritedMqbNumpy)
    }
}
