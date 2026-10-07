use crate::{
    json::{field, number, optional_text, text},
    Error,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    None,
    Left,
    Right,
    Straight,
    SlightLeft,
    SlightRight,
}

pub fn string_to_direction(value: &str) -> Direction {
    for (text, direction, slight) in [
        ("left", Direction::Left, Direction::SlightLeft),
        ("right", Direction::Right, Direction::SlightRight),
        ("straight", Direction::Straight, Direction::Straight),
    ] {
        if value.contains(text) {
            return if value.contains("slight") {
                slight
            } else {
                direction
            };
        }
    }
    Direction::None
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    pub active: bool,
    pub directions: Vec<Direction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_direction: Option<Direction>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BannerInstruction {
    pub show_full: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuver_primary_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuver_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuver_modifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuver_secondary_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lanes: Option<Vec<Lane>>,
}

pub fn parse_banner_instructions(
    banners: &Value,
    distance: f64,
) -> Result<Option<BannerInstruction>, Error> {
    let banners = banners
        .as_array()
        .ok_or(Error::Field("bannerInstructions"))?;
    let Some(mut current) = banners.first() else {
        return Ok(None);
    };
    for banner in banners {
        if distance
            < number(
                field(banner, "distanceAlongGeometry")?,
                "distanceAlongGeometry",
            )?
        {
            current = banner;
        }
    }
    let primary = field(current, "primary")?;
    let secondary = current.get("secondary").filter(|value| !value.is_null());
    let sub = current.get("sub").filter(|value| !value.is_null());
    Ok(Some(BannerInstruction {
        show_full: distance
            < number(
                field(current, "distanceAlongGeometry")?,
                "distanceAlongGeometry",
            )?,
        maneuver_primary_text: optional_text(primary, "text")?,
        maneuver_type: optional_text(primary, "type")?,
        maneuver_modifier: optional_text(primary, "modifier")?,
        maneuver_secondary_text: secondary
            .map(|value| text(field(value, "text")?, "text"))
            .transpose()?,
        lanes: sub.map(lanes).transpose()?,
    }))
}

fn lanes(sub: &Value) -> Result<Vec<Lane>, Error> {
    let components = field(sub, "components")?
        .as_array()
        .ok_or(Error::Field("components"))?;
    let mut lanes = Vec::new();
    for component in components {
        if field(component, "type")?.as_str() != Some("lane") {
            continue;
        }
        let directions = field(component, "directions")?
            .as_array()
            .ok_or(Error::Field("directions"))?;
        let directions = directions
            .iter()
            .map(|direction| {
                direction
                    .as_str()
                    .map(string_to_direction)
                    .ok_or(Error::Field("directions"))
            })
            .collect::<Result<_, _>>()?;
        lanes.push(Lane {
            active: field(component, "active")?
                .as_bool()
                .ok_or(Error::Field("active"))?,
            directions,
            active_direction: optional_text(component, "active_direction")?
                .as_deref()
                .map(string_to_direction),
        });
    }
    Ok(lanes)
}
