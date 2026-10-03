use super::{
    state::{float, Bus, State},
    Error, ALT_GEAR,
};
use openpilot_cereal::car_capnp::car_state;
impl State {
    pub(super) fn meb(&mut self, ret: &mut car_state::Builder<'_>, now: u64) -> Result<(), Error> {
        let mut values = [0.; 4];
        for (index, key) in [
            "VL_Radgeschw",
            "VR_Radgeschw",
            "HL_Radgeschw",
            "HR_Radgeschw",
        ]
        .into_iter()
        .enumerate()
        {
            values[index] = float(
                self.signal(Bus::Pt, ("ESC_51", key), now)?
                    * ((1. / 3.6) * self.config.wheel_factor),
            )?;
        }
        let mut wheels = ret.reborrow().init_wheel_speeds();
        wheels.set_fl(values[0]);
        wheels.set_fr(values[1]);
        wheels.set_rl(values[2]);
        wheels.set_rr(values[3]);
        let raw = float(
            (f64::from(values[0])
                + f64::from(values[1])
                + f64::from(values[2])
                + f64::from(values[3]))
                / 4.,
        )?;
        ret.set_v_ego_raw(raw);
        let [speed, accel] = self.speed.update(f64::from(raw));
        ret.set_v_ego(float(speed)?);
        ret.set_a_ego(float(accel)?);
        ret.set_standstill(raw == 0.);
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
        self.extras.steering_pressed_cnt =
            if f64::from(torque.abs()) > self.config.driver_allowance() {
                self.extras.steering_pressed_cnt.saturating_add(1).min(6)
            } else {
                0
            };
        ret.set_steering_pressed(self.extras.steering_pressed_cnt > 5);
        self.extras.curvature =
            -self.signed(Bus::Pt, ("QFK_01", "Curvature", "Curvature_VZ"), now)?;
        ret.set_steering_curvature(float(self.extras.curvature)?);
        self.extras.left_blinker_active =
            self.signal(Bus::Pt, ("Blinkmodi_02", "BM_links"), now)? != 0.;
        self.extras.right_blinker_active =
            self.signal(Bus::Pt, ("Blinkmodi_02", "BM_rechts"), now)? != 0.;
        ret.set_yaw_rate(float(
            -self.signed(Bus::Pt, ("ESC_50", "Yaw_Rate", "Yaw_Rate_Sign"), now)?
                * (std::f64::consts::PI / 180.),
        )?);
        let value = self.signal(Bus::Pt, ("QFK_01", "LatCon_HCA_Status"), now)?;
        let status = self.hca_status(value)?.map(|s| s.to_uppercase());
        let gear = self.signal(
            Bus::Pt,
            (
                if self.config.flags & ALT_GEAR != 0 {
                    "Gateway_73"
                } else {
                    "Getriebe_11"
                },
                "GE_Fahrstufe",
            ),
            now,
        )?;
        self.hca_faults(ret, status.as_deref(), gear != 0.);
        ret.set_gear_shifter(self.gear(gear)?);
        ret.set_gas_pressed(self.signal(Bus::Pt, ("Motor_51", "Accel_Pedal_Pressure"), now)? > 0.);
        ret.set_brake_pressed(self.signal(Bus::Pt, ("Motor_14", "MO_Fahrer_bremst"), now)? != 0.);
        ret.set_brake(float(self.signal(
            Bus::Pt,
            ("ESC_51", "Brake_Pressure"),
            now,
        )?)?);
        let epb = self.signal(Bus::Pt, ("ESC_50", "EPB_Status"), now)?;
        ret.set_parking_brake(epb == 1. || epb == 4.);
        let door_message = if self.signal(Bus::Pt, ("Gateway_72", "ZV_02_alt"), now)? != 0. {
            "ZV_02"
        } else {
            "Gateway_72"
        };
        let mut door = false;
        for key in [
            "ZV_FT_offen",
            "ZV_BT_offen",
            "ZV_HFS_offen",
            "ZV_HBFS_offen",
            "ZV_HD_offen",
        ] {
            door |= self.signal(Bus::Pt, (door_message, key), now)? != 0.;
        }
        ret.set_door_open(door);
        ret.set_seatbelt_unlatched(
            self.signal(Bus::Pt, ("Airbag_02", "AB_Gurtschloss_FA"), now)? != 3.,
        );
        self.meb_controls(ret, now)
    }
}
