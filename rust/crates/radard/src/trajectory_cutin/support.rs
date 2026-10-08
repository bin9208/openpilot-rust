use super::constants::*;
use crate::{
    math::{finite, maximum},
    model::{Model, VisionLead},
    path::{Path, Projection},
    point::{velocity_in_ego_frame, Point},
    Error, Lead,
};

fn first(values: &Option<Vec<f64>>, fallback: f64) -> Result<f64, Error> {
    let Some(values) = values else {
        return Ok(fallback);
    };
    values
        .first()
        .copied()
        .map(|value| finite(value, fallback))
        .ok_or(Error::Contract("trajectory vision field is empty"))
}

pub fn vision_supports(point: &Point, model: &Model, precise: bool) -> Result<bool, Error> {
    for lead in &model.leads {
        let probability = finite(lead.probability, 0.);
        if probability < if precise { 0.90 } else { 0.35 } {
            continue;
        }
        let distance = first(&lead.x, 0.)? - 1.52;
        let lateral = -first(&lead.y, 0.)?;
        let velocity = first(&lead.v, point.v_lead)?;
        let x_std = maximum(1., first(&lead.x_std, 1.)?);
        let y_std = maximum(0.5, first(&lead.y_std, 0.5)?);
        let v_std = maximum(1., first(&lead.v_std, 1.)?);
        if (point.d_rel - distance).abs()
            <= maximum(
                maximum(4., 0.20 * point.d_rel),
                if precise { 0. } else { 2. * x_std },
            )
            && (point.y_rel - lateral).abs()
                <= if precise {
                    1.5
                } else {
                    maximum(1.5, 2. * y_std)
                }
            && (point.v_lead - velocity).abs() <= maximum(5., 2. * v_std)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn vision_brackets(
    point: &Point,
    front: Option<&Point>,
    vision: Option<VisionLead>,
    primary: Option<&Lead>,
    path: &Path,
) -> bool {
    point.corner()
        && front
            .zip(vision)
            .zip(primary)
            .is_some_and(|((front, vision), primary)| {
                front.source == "frontRadar"
                    && vision.probability >= 0.90
                    && primary.status
                    && 0.5 < point.d_rel
                    && point.d_rel <= 8.
                    && front.d_rel + 0.5 < vision.d_rel
                    && vision.d_rel < finite(primary.d_rel, 0.) - 0.5
                    && vision.d_rel - front.d_rel <= 6.
                    && front.y_rel.abs() <= PAIRED_CLOSE_BODY_HALF_WIDTH_M
                    && path.project(vision.d_rel, vision.y_rel).d_path.abs()
                        <= EGO_PATH_HALF_WIDTH_M
                    && (vision.velocity - front.v_lead).abs() <= 2.
                    && (vision.velocity - point.v_lead).abs() <= 2.
            })
}

pub fn reported_inward(point: &Point, projection: Projection, yaw: f64) -> f64 {
    let [x, y] = velocity_in_ego_frame(point, yaw);
    let normal = -projection.tangent_y * x + projection.tangent_x * y;
    let side = 1_f64.copysign(if projection.d_path != 0. {
        projection.d_path
    } else if point.y_rel != 0. {
        point.y_rel
    } else {
        1.
    });
    maximum(0., -side * normal)
}

pub fn existence_supported(point: &Point, history: f64, vision: bool, cross: bool) -> bool {
    if !point.measured {
        vision && history >= 0.10
    } else if vision || cross {
        history >= 0.10
    } else if point.source == "frontRadar" && point.radar_track_state == 1 {
        history >= TENTATIVE_FRONT_HISTORY_S
    } else {
        history >= MIN_EXISTENCE_HISTORY_S
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_uncertainty_defaults_but_explicit_empty_array_is_invalid() {
        let point = Point {
            d_rel: 11.,
            v_lead: 7.,
            ..Point::default()
        };
        let source = r#"{"leads":[{"probability":0.99,"x":[12.52],"y":[0.0],"v":[7.0]}]}"#;
        let model: Model = serde_json::from_str(source).expect("test model decodes");
        assert!(
            vision_supports(&point, &model, false).expect("missing fields have source defaults")
        );
        let mut explicit = model;
        explicit.leads[0].x_std = Some(Vec::new());
        assert!(vision_supports(&point, &explicit, false).is_err());
    }
}
