use super::Input;
use crate::{live::Error, models::DebugPlotSnapshot};

impl Input {
    pub(in crate::live) fn debug_plot_snapshot(&self) -> Result<Option<DebugPlotSnapshot>, Error> {
        let mode = self.debug.plot_mode;
        if mode <= 0 {
            return Ok(None);
        }
        let car = self.data("carState")?;
        let long = self.data("longitudinalPlan")?;
        let control = self.data("carControl")?;
        let controls = self.data("controlsState")?;
        let model = self.data("modelV2")?;
        let radar = self.data("radarState")?;
        let params = self.data("liveParameters")?;
        let acceleration = car.number("aEgo", 0.0);
        let speed = car.number("vEgo", 0.0);
        let accel_target = long.field("accels").index(0).float().unwrap_or(0.0);
        let speed_target = long.field("speeds").index(0).float().unwrap_or(0.0);
        let actuators = control.field("actuators");
        let lead = radar.field("leadOne");
        let torque = controls.field("lateralControlState").field("torqueState");
        let (values, title) = match mode {
            1 => (
                [acceleration, accel_target, actuators.number("accel", 0.0)],
                "1.Accel (Y:a_ego, G:a_target, O:a_out)",
            ),
            2 => (
                [speed_target, speed, acceleration],
                "2.Speed/Accel(Y:speed_0, G:v_ego, O:a_ego)",
            ),
            3 => (
                [
                    model
                        .field("position")
                        .field("x")
                        .index(32)
                        .float()
                        .unwrap_or(0.0),
                    model
                        .field("velocity")
                        .field("x")
                        .index(32)
                        .float()
                        .unwrap_or(0.0),
                    model
                        .field("velocity")
                        .field("x")
                        .index(0)
                        .float()
                        .unwrap_or(0.0),
                ],
                "3.Model(Y:pos_32, G:vel_32, O:vel_0)",
            ),
            4 => (
                [
                    accel_target,
                    lead.number("aLeadK", 0.0),
                    lead.number("vRel", 0.0),
                ],
                "4.Lead(Y:accel, G:a_leadK, O:v_rel)",
            ),
            5 => (
                [
                    acceleration,
                    lead.number("aLead", 0.0),
                    lead.number("jLead", 0.0),
                ],
                "5.Lead(Y:a_ego, G:a_lead, O:j_lead)",
            ),
            6 => (
                [
                    torque.number("actualLateralAccel", 0.0) * 10.0,
                    torque.number("desiredLateralAccel", 0.0) * 10.0,
                    torque.number("output", 0.0) * 10.0,
                ],
                "6.Steer(Y:actual, G:desire, O:output) *10",
            ),
            7 => (
                [
                    car.number("steeringAngleDeg", 0.0),
                    actuators.number("steeringAngleDeg", 0.0),
                    params.number("angleOffsetDeg", 0.0) * 10.0,
                ],
                "7.SteerA(Y:Actual, G:Target, O:Offset*10)",
            ),
            8 => {
                let curvature = actuators.number("curvature", 0.0) * 10000.0;
                ([curvature; 3], "8.Curvature(*10000)")
            }
            _ => ([0.0; 3], "no data"),
        };
        Ok(Some(DebugPlotSnapshot {
            mode,
            title: title.into(),
            values,
        }))
    }
}
