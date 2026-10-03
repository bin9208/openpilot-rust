use super::{lead::Lead, Error};
use openpilot_cereal::log_capnp::{model_data_v2, radar_state};

#[derive(Default)]
pub struct Model {
    pub leads: Vec<Lead>,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub y_std: Option<f64>,
    pub change_available: [bool; 2],
    pub changing: u8,
    pub desire: u16,
}

impl Model {
    pub fn read(
        model: Option<model_data_v2::Reader<'_>>,
        radar: Option<radar_state::Reader<'_>>,
    ) -> Result<Self, Error> {
        let mut ret = Self::default();
        if let Some(model) = model {
            let position = model.get_position()?;
            ret.x = position.get_x()?.iter().map(f64::from).collect();
            ret.y = position.get_y()?.iter().map(f64::from).collect();
            let std = position.get_y_std()?;
            ret.y_std = if std.len() > 10 {
                Some(f64::from(std.get(10)))
            } else {
                None
            };
            let meta = model.get_meta()?;
            ret.change_available = [
                meta.get_lane_change_available_left(),
                meta.get_lane_change_available_right(),
            ];
            ret.desire = meta.get_desire()?.into();
            let desire = meta.get_desire_state()?;
            if desire.len() > 4 {
                for index in 1..=4 {
                    if desire.get(index) > 0.9 {
                        ret.changing = u8::try_from(index).map_err(|_| Error::Numeric)?;
                    }
                }
            }
        }
        if let Some(radar) = radar {
            for lead in [radar.get_lead_one()?, radar.get_lead_two()?] {
                ret.leads.push(Lead {
                    status: lead.get_status(),
                    radar: lead.get_radar(),
                    radar_track_id: i64::from(lead.get_radar_track_id()),
                    d_rel: f64::from(lead.get_d_rel()),
                    y_rel: f64::from(lead.get_y_rel()),
                    v_rel: f64::from(lead.get_v_rel()),
                });
            }
        }
        Ok(ret)
    }
}
