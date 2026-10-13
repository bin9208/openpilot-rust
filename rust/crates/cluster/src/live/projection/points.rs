use super::Data;
use crate::{
    config::RADAR_TO_CAMERA_M,
    models::{DetectedVehicle, ModelPathPoint},
};

pub(super) fn line_points(line: Data<'_>) -> Vec<ModelPathPoint> {
    let ys = line.field("y");
    let mut previous = -1.0;
    line.field("x")
        .items()
        .into_iter()
        .enumerate()
        .filter_map(|(index, x)| {
            let forward = x.float()?;
            let lateral = ys.index(index).float()?;
            if !(0.0..=160.0).contains(&forward)
                || lateral.abs() > 24.0
                || forward <= previous + 0.01
            {
                return None;
            }
            previous = forward;
            Some(ModelPathPoint {
                forward_m: forward,
                lateral_m: lateral,
                ..ModelPathPoint::default()
            })
        })
        .collect()
}
pub(super) fn path_points(model: Data<'_>) -> Vec<ModelPathPoint> {
    let position = model.field("position");
    let ys = position.field("y");
    let mut previous = -1.0;
    position
        .field("x")
        .items()
        .into_iter()
        .enumerate()
        .filter_map(|(index, x)| {
            let forward = x.float()?;
            let lateral = ys.index(index).float()?;
            if !(0.0..=140.0).contains(&forward)
                || lateral.abs() > 18.0
                || forward <= previous + 0.01
            {
                return None;
            }
            previous = forward;
            Some(ModelPathPoint {
                forward_m: forward,
                lateral_m: lateral,
                lateral_std_m: position.field("yStd").index(index).float(),
                speed_mps: model.field("velocity").field("x").index(index).float(),
                accel_mps2: model.field("acceleration").field("x").index(index).float(),
                orientation_rad: model.field("orientation").field("z").index(index).float(),
                orientation_rate_rps: model
                    .field("orientationRate")
                    .field("z")
                    .index(index)
                    .float(),
            })
        })
        .collect()
}
pub(super) fn model_leads(model: Data<'_>) -> Vec<DetectedVehicle> {
    let speed = model.field("velocity").field("x").index(0).float();
    model
        .field("leadsV3")
        .items()
        .into_iter()
        .enumerate()
        .filter_map(|(index, lead)| {
            let probability = lead.number("prob", 0.0).clamp(0.0, 1.0);
            if probability < 0.5 {
                return None;
            }
            let longitudinal = lead.field("x").index(0).float()? - RADAR_TO_CAMERA_M;
            let lateral = lead.field("y").index(0).float()?;
            if longitudinal <= 0.2 || longitudinal >= 180.0 || lateral.abs() > 8.0 {
                return None;
            }
            let velocity = lead.field("v").index(0).float();
            let relative = velocity.zip(speed).map(|(lead, ego)| lead - ego);
            Some(DetectedVehicle {
                label: format!("M{}", index + 1),
                longitudinal_m: longitudinal,
                lateral_m: lateral.clamp(-8.0, 8.0),
                source: "modelV2.leadsV3".into(),
                probability,
                relative_speed_mps: relative,
                absolute_speed_kph: velocity.map(|speed| (speed * 3.6).max(0.0)),
                acceleration_mps2: lead.field("a").index(0).float(),
                primary: index == 0,
                ttc_s: ttc(longitudinal, relative),
                x_std_m: lead.field("xStd").index(0).float(),
                y_std_m: lead.field("yStd").index(0).float(),
                ..DetectedVehicle::default()
            })
        })
        .collect()
}
pub(super) fn ttc(longitudinal: f64, relative: Option<f64>) -> Option<f64> {
    let speed = relative?;
    if longitudinal <= 0.0 || speed >= -0.15 {
        return None;
    }
    Some((longitudinal / (-speed).max(0.15)).clamp(0.0, 99.0))
}
