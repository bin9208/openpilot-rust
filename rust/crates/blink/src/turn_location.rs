//! Optional turn passage evidence from raw vehicle GPS, never route-snapped
//! pose. Port of carrot's `tools/jetlink/blink_turn_location.py`.

/// `(latitude, longitude, receive time, horizontal accuracy)`.
pub type Position = (f64, f64, f64, f64);
/// `(latitude, longitude, navigation session id)`.
pub type Target = (f64, f64, String);

pub fn coordinates(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 85. && lon.abs() <= 180.
}

/// Local east/north metres of `point` from `origin`.
pub fn offset(origin: (f64, f64), point: (f64, f64)) -> (f64, f64) {
    let (lat, lon) = origin;
    (
        (point.1 - lon) * 111_320. * lat.to_radians().cos(),
        (point.0 - lat) * 110_540.,
    )
}

#[derive(Clone, Debug, Default)]
pub struct TurnLocation {
    pub target: Option<Target>,
    closest: f64,
    previous: Option<(f64, f64, f64)>,
    straight_since: Option<f64>,
    passed: bool,
    cancelled: bool,
    incoming: f64,
}

impl TurnLocation {
    pub fn empty() -> Self {
        Self {
            closest: f64::INFINITY,
            ..Self::default()
        }
    }

    pub fn new(
        position: Option<Position>,
        target: Option<Target>,
        heading: Option<f64>,
        distance: Option<f64>,
    ) -> Self {
        let mut this = Self::empty();
        let (Some(position), Some(target), Some(heading), Some(distance)) =
            (position, target, heading, distance)
        else {
            return this;
        };
        if !heading.is_finite() || !distance.is_finite() || !(distance > 0. && distance <= 150.) {
            return this;
        }
        let (east, north) = offset((target.0, target.1), (position.0, position.1));
        let radius = east.hypot(north);
        // The point must be ahead and consistent with the TBT distance; a
        // curved approach or unrelated instruction is not a straight crossing.
        let bearing = (-east).atan2(-north);
        let error = (bearing - heading).sin().atan2((bearing - heading).cos());
        if radius < 8. || (radius - distance).abs() > 25. || error.abs() > 25f64.to_radians() {
            return this;
        }
        this.target = Some(target);
        this.incoming = heading;
        this
    }

    /// Returns `(passed, straight_cancelled)`.
    pub fn update(
        &mut self,
        position: Option<Position>,
        heading: Option<f64>,
        speed: f64,
        angle: f64,
        turned: bool,
    ) -> (bool, bool) {
        let Some(target) = &self.target else {
            return (self.passed, self.cancelled);
        };
        if self.cancelled {
            return (self.passed, self.cancelled);
        }
        let (Some(position), Some(heading)) = (position, heading) else {
            self.reset_motion();
            return (false, false);
        };
        if !(heading.is_finite() && speed.is_finite() && angle.is_finite()) {
            self.reset_motion();
            return (false, false);
        }
        let (east, north) = offset((target.0, target.1), (position.0, position.1));
        let radius = east.hypot(north);
        let stamp = position.2;
        if let Some((old_stamp, old_east, old_north)) = self.previous {
            if stamp == old_stamp {
                // A repeated receipt is not new motion evidence.
                return (self.passed, false);
            }
            let dt = stamp - old_stamp;
            if !(dt > 0. && dt <= 2.)
                || (east - old_east).hypot(north - old_north)
                    > 10f64.max(speed.max(0.) * dt + 2. * position.3)
            {
                self.reset_motion();
                return (false, false);
            }
        }
        self.previous = Some((stamp, east, north));
        self.closest = self.closest.min(radius);
        let along = east * self.incoming.sin() + north * self.incoming.cos();
        let cross = east * self.incoming.cos() - north * self.incoming.sin();
        self.passed = self.closest <= 15. && radius >= self.closest + 15.;
        let turn = heading - self.incoming;
        let rotation = turn.sin().atan2(turn.cos()).abs();
        let straight = !turned
            && self.closest <= 15.
            && along >= 20. + 2. * position.3
            && cross.abs() <= 10.
            && rotation < 15f64.to_radians()
            && angle.abs() <= 10.
            && speed >= 2.;
        self.straight_since = if straight {
            Some(self.straight_since.unwrap_or(stamp))
        } else {
            None
        };
        self.cancelled = self.straight_since.is_some_and(|since| stamp - since >= 1.);
        (self.passed, self.cancelled)
    }

    fn reset_motion(&mut self) {
        self.previous = None;
        self.straight_since = None;
        self.closest = f64::INFINITY;
        self.passed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_crossing_cancels_and_a_turn_passes() {
        let target = (37.5, 127.0, "s".to_owned());
        // Approach from the south, heading north (0 rad), 60 m before the point.
        let start = (37.5 - 60. / 110_540., 127.0, 0., 3.);
        let mut straight =
            TurnLocation::new(Some(start), Some(target.clone()), Some(0.), Some(60.));
        assert!(straight.target.is_some());
        let mut result = (false, false);
        for i in 1..=12 {
            let north = -60. + f64::from(i) * 10.;
            let position = (37.5 + north / 110_540., 127.0, f64::from(i), 3.);
            result = straight.update(Some(position), Some(0.), 10., 0., false);
        }
        assert!(result.1, "driving straight through cancels");

        // Not ahead of the heading: not a usable target.
        let behind = TurnLocation::new(
            Some(start),
            Some(target),
            Some(std::f64::consts::PI),
            Some(60.),
        );
        assert!(behind.target.is_none());
    }
}
