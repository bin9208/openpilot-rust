use super::{
    state::{float, Bus, State, Stock},
    Error, MEB_GEN2, STOCK_EA_PRESENT, STOCK_KLR_PRESENT,
};
use openpilot_cereal::car_capnp::{
    car_params::NetworkLocation,
    car_state::{self, GearShifter},
};
impl State {
    pub(super) fn meb_controls(
        &mut self,
        ret: &mut car_state::Builder<'_>,
        now: u64,
    ) -> Result<(), Error> {
        if self.config.bsm {
            let (bus, left, right) = if self.config.flags & MEB_GEN2 != 0 {
                (
                    Bus::Pt,
                    ["Blind_Spot_Info_Driver", "Blind_Spot_Warn_Driver"],
                    ["Blind_Spot_Info_Passenger", "Blind_Spot_Warn_Passenger"],
                )
            } else {
                (
                    Bus::External,
                    ["Blind_Spot_Info_Left", "Blind_Spot_Warn_Left"],
                    ["Blind_Spot_Info_Right", "Blind_Spot_Warn_Right"],
                )
            };
            ret.set_left_blindspot(
                self.signal(bus, ("MEB_Side_Assist_01", left[0]), now)? != 0.
                    || self.signal(bus, ("MEB_Side_Assist_01", left[1]), now)? != 0.,
            );
            ret.set_right_blindspot(
                self.signal(bus, ("MEB_Side_Assist_01", right[0]), now)? != 0.
                    || self.signal(bus, ("MEB_Side_Assist_01", right[1]), now)? != 0.,
            );
        }
        self.extras.ldw_stock_values = Some(self.copied(Bus::Cam, "LDW_02", now)?);
        if self.config.flags & STOCK_EA_PRESENT != 0 {
            self.extras.ea_hud_stock_values = self.copied(Bus::Cam, "EA_02", now)?;
            self.extras.ea_control_stock_values = self.copied(Bus::Cam, "EA_01", now)?;
            ret.set_car_faulted_non_critical(matches!(
                self.signal(Bus::Cam, ("EA_01", "EA_Funktionsstatus"), now)?,
                3. | 4. | 5. | 6.
            ));
        }
        self.extras.acc_type = Some(match self.config.network {
            NetworkLocation::Gateway => self.signal(Bus::External, ("ACC_18", "ACC_Typ"), now)?,
            NetworkLocation::FwdCamera => 2.,
        });
        self.extras.eps_stock_values = Stock::Values(self.copied(Bus::Pt, "LH_EPS_03", now)?);
        self.extras.klr_stock_values = if self.config.flags & STOCK_KLR_PRESENT != 0 {
            self.copied(Bus::Pt, "KLR_01", now)?
        } else {
            Default::default()
        };
        self.extras.travel_assist_available =
            self.signal(Bus::Cam, ("TA_01", "Travel_Assist_Available"), now)? != 0.;
        self.extras.esp_hold_confirmation =
            self.signal(Bus::Pt, ("ESC_50", "Motion_State"), now)? == 3.;
        self.extras.long_control_inhibit =
            self.signal(Bus::Pt, ("VMM_02", "Long_Control_Inhibit"), now)? == 2.;
        let status = self.signal(Bus::Pt, ("Motor_51", "TSK_Status"), now)?;
        let available = matches!(status, 2. | 3. | 4. | 5.);
        let enabled = matches!(status, 3. | 4. | 5.);
        let mut fault = matches!(status, 6. | 7.);
        if self.extras.long_control_inhibit
            || ret.reborrow_as_reader().get_parking_brake()
                && ret.reborrow_as_reader().get_gear_shifter()? != GearShifter::Drive
        {
            self.extras.cruise_recovery_timer = self.extras.frame;
            fault = false;
        } else if self
            .extras
            .frame
            .saturating_sub(self.extras.cruise_recovery_timer)
            < 100
        {
            fault = false;
        }
        ret.set_acc_faulted(fault);
        let nonadaptive = self.signal(Bus::Pt, ("Motor_51", "TSK_Limiter_ausgewaehlt"), now)? != 0.;
        let speed = if self.config.pcm {
            let raw = self
                .signal(Bus::External, ("MEB_ACC_01", "ACC_Wunschgeschw_02"), now)?
                .round_ties_even();
            float(raw * (1. / 3.6))?
        } else {
            0.
        };
        let mut cruise = ret.reborrow().init_cruise_state();
        cruise.set_available(available);
        cruise.set_enabled(enabled);
        cruise.set_standstill(self.config.pcm && self.extras.esp_hold_confirmation);
        cruise.set_non_adaptive(nonadaptive);
        cruise.set_speed(if speed > 90. { 0. } else { speed });
        let left = self.signal(Bus::Pt, ("SMLS_01", "BH_Blinker_li"), now)? != 0.;
        let right = self.signal(Bus::Pt, ("SMLS_01", "BH_Blinker_re"), now)? != 0.;
        self.blinkers(ret, 240, [left, right]);
        let latching = self.signal(Bus::Pt, ("GRA_ACC_01", "GRA_Typ_Hauptschalter"), now)? == 0.;
        self.buttons(ret, now, latching)?;
        ret.set_cruise_speed_big_step(
            self.signal(Bus::Pt, ("GRA_ACC_01", "GRA_Tip_Stufe_2"), now)? != 0.,
        );
        self.extras.gra_stock_values = Some(self.copied(Bus::Pt, "GRA_ACC_01", now)?);
        ret.set_esp_disabled(self.signal(Bus::Pt, ("ESP_21", "ESP_Tastung_passiv"), now)? != 0.);
        ret.set_esp_active(self.signal(Bus::Pt, ("ESP_21", "ESP_Eingriff"), now)? != 0.);
        Ok(())
    }
}
