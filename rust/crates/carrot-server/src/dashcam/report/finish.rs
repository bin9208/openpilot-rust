use super::{
    format::{self, number},
    state::State,
};
use crate::{Error, Value};

impl State {
    pub fn finish(self, route: Value, title: Value, segments: usize) -> Result<Value, Error> {
        let total = self.auto_enabled + self.manual;
        let average = if total > 0.0 {
            self.distance / total
        } else {
            0.0
        };
        let warnings = self.warnings();
        let (hard_accel, over_accel) = self.accel.events()?;
        let (hard_decel, over_decel) = self.decel.events()?;
        let disengages = self
            .disengages
            .into_iter()
            .map(|(wall, cause)| {
                Ok(Value::object([
                    ("clock", Value::text(&format::clock(wall)?)),
                    ("cause", Value::text(cause)),
                ]))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("route", route),
            ("title", title),
            ("source", Value::text(self.source)),
            ("segments", Value::integer(segments)),
            ("hasData", Value::Bool(self.used_log)),
            (
                "time",
                Value::object([
                    ("totalHms", Value::text(&format::hms(total)?)),
                    (
                        "autoEnabledHms",
                        Value::text(&format::hms(self.auto_enabled)?),
                    ),
                    (
                        "autoActiveHms",
                        Value::text(&format::hms(self.auto_active)?),
                    ),
                    ("manualHms", Value::text(&format::hms(self.manual)?)),
                    ("manualGasMs", Value::text(&format::ms(self.manual_gas)?)),
                    (
                        "manualBrakeMs",
                        Value::text(&format::ms(self.manual_brake)?),
                    ),
                    ("stopMs", Value::text(&format::ms(self.stop)?)),
                    ("steerOverrideMs", Value::text(&format::ms(self.steer)?)),
                    ("startClock", Value::text(&format::clock(self.first_wall)?)),
                    ("endClock", Value::text(&format::clock(self.last_wall)?)),
                    (
                        "autoRatioPct",
                        number(
                            if total > 0.0 {
                                100.0 * self.auto_enabled / total
                            } else {
                                0.0
                            },
                            1,
                        ),
                    ),
                    (
                        "manualRatioPct",
                        number(
                            if total > 0.0 {
                                100.0 * self.manual / total
                            } else {
                                0.0
                            },
                            1,
                        ),
                    ),
                    ("totalSec", number(total, 1)),
                    ("autoSec", number(self.auto_enabled, 1)),
                    ("manualSec", number(self.manual, 1)),
                    ("manualGasSec", number(self.manual_gas, 1)),
                    ("manualBrakeSec", number(self.manual_brake, 1)),
                ]),
            ),
            (
                "distance",
                Value::object([
                    ("totalKm", number(self.distance / 1000.0, 2)),
                    ("autoKm", number(self.auto_distance / 1000.0, 2)),
                    (
                        "manualKm",
                        number((self.distance - self.auto_distance).max(0.0) / 1000.0, 2),
                    ),
                    ("avgSpeedKmh", number(average * 3.6, 1)),
                    ("maxSpeedKmh", number(self.max_speed * 3.6, 1)),
                ]),
            ),
            (
                "events",
                Value::object([
                    ("hardAccel", hard_accel),
                    ("overAccel", over_accel),
                    ("hardDecel", hard_decel),
                    ("overDecel", over_decel),
                ]),
            ),
            (
                "extras",
                Value::object([
                    ("disengageCount", Value::integer(self.disengage_count)),
                    (
                        "disengageCauses",
                        Value::Object(
                            self.causes
                                .into_iter()
                                .map(|(cause, count)| {
                                    (
                                        cause.chars().map(u32::from).collect(),
                                        Value::integer(count),
                                    )
                                })
                                .collect(),
                        ),
                    ),
                    ("disengageItems", Value::Array(disengages)),
                    ("stopCount", Value::integer(self.stop_count)),
                    ("steerOverrideCount", Value::integer(self.steer_count)),
                    ("cornerCount", Value::integer(self.corner_count)),
                    ("maxLatAccel", number(self.max_lat, 2)),
                    ("warnCounts", warnings),
                ]),
            ),
        ]))
    }
}
