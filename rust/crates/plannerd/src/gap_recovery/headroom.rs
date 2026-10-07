use openpilot_control_policy::math::{maximum, minimum};

pub(super) struct Headroom {
    pub reservoir: f64,
    pub extra: f64,
}
pub(super) struct Recovery {
    pub dt: f64,
    pub lead_speed: f64,
    pub tau: f64,
    pub cap: f64,
}

pub(super) fn advance(state: Headroom, target: Option<f64>, step: Recovery) -> Headroom {
    let mut reservoir = minimum(step.cap, state.reservoir);
    let mut extra = minimum(step.cap, state.extra);
    let strength = minimum(1., maximum(0., (step.lead_speed - 0.3) / (5. - 0.3)));
    let k = 2. * strength / step.tau;
    if let Some(target) = target {
        let charge_time = minimum(
            step.dt,
            maximum(0., minimum(step.cap, target) - reservoir) / 0.5,
        );
        if k > 0. {
            let following = -(-k * charge_time).exp_m1();
            extra += (reservoir - extra) * following + 0.5 * (charge_time - following / k);
        }
        reservoir += 0.5 * charge_time;
        if k > 0. {
            extra += (reservoir - extra) * -(-k * (step.dt - charge_time)).exp_m1();
        }
        return Headroom {
            reservoir,
            extra: minimum(step.cap, maximum(0., extra)),
        };
    }
    if k <= 0. {
        return Headroom { reservoir, extra };
    }
    let z = k * step.dt;
    let decay = (-z).exp();
    Headroom {
        reservoir: reservoir * decay,
        extra: (extra + z * reservoir) * decay,
    }
}
