use super::{
    state::{Snapshot, State, Values},
    Error,
};
use crate::core::VehicleLog;
use crate::query::DiagnosticLevel;
use num_traits::ToPrimitive;
use openpilot_can::parser::Parser;
use openpilot_cereal::car_capnp::car_state;
#[derive(Clone, Copy)]
pub(super) enum Bus {
    Pt,
    Cam,
    External,
}
pub(super) fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
pub(super) fn required(
    values: &Values,
    names: &[&'static str],
) -> Result<Vec<(&'static str, f64)>, Error> {
    names
        .iter()
        .map(|name| {
            Ok((
                *name,
                *values
                    .get(*name)
                    .ok_or_else(|| Error::Signal((*name).into()))?,
            ))
        })
        .collect()
}
impl State {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            speed_filter: self.speed.state(),
            cluster_seen: self.cluster_seen,
        }
    }
    pub(super) fn parser(&mut self, bus: Bus) -> &mut Parser {
        match bus {
            Bus::Pt => &mut self.pt,
            Bus::Cam => &mut self.camera,
            Bus::External => {
                if self.config.external_bus() == 0 {
                    &mut self.pt
                } else {
                    &mut self.camera
                }
            }
        }
    }
    pub(super) fn signal(&mut self, bus: Bus, id: (&str, &str), now: u64) -> Result<f64, Error> {
        Ok(self.parser(bus).signal_lazy(id.0, id.1, now)?)
    }
    pub(super) fn copied(&mut self, bus: Bus, name: &str, now: u64) -> Result<Values, Error> {
        let parser = self.parser(bus);
        let keys = parser
            .dbc
            .message(name)?
            .signals
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>();
        keys.into_iter()
            .map(|key| Ok((key.clone(), parser.signal_lazy(name, &key, now)?)))
            .collect()
    }
    pub(super) fn signed(
        &mut self,
        bus: Bus,
        id: (&str, &str, &str),
        now: u64,
    ) -> Result<f64, Error> {
        let value = self.signal(bus, (id.0, id.1), now)?;
        let sign = self.signal(bus, (id.0, id.2), now)?;
        if sign == 0. {
            Ok(value)
        } else if sign == 1. {
            Ok(-value)
        } else {
            Err(Error::Numeric)
        }
    }
    pub(super) fn gear(&self, value: f64) -> Result<car_state::GearShifter, Error> {
        let value = value.to_i64().ok_or(Error::Numeric)?;
        Ok(crate::state_helpers::parse_gear(
            self.gears
                .as_ref()
                .ok_or(Error::Stock("shifter_values"))?
                .get(&value)
                .map(String::as_str),
        ))
    }
    pub(super) fn hca_status(&self, value: f64) -> Result<Option<String>, Error> {
        let value = value.to_i64().ok_or(Error::Numeric)?;
        Ok(self.hca.get(&value).cloned())
    }
    pub(super) fn drain_logs(&mut self) {
        for parser in [&mut self.pt, &mut self.camera] {
            self.logs.extend(
                std::mem::take(&mut parser.diagnostics)
                    .into_iter()
                    .map(|d| VehicleLog {
                        level: DiagnosticLevel::Warning,
                        message: d.message,
                    }),
            );
        }
    }
}
