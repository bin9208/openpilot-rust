use crate::{numbers, Error};
use num_traits::ToPrimitive;
use openpilot_logmessaged::{JsonValue, JsonView};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Config {
    pub width: u32,
    pub height: u32,
    pub poly_left: Vec<[i32; 2]>,
    pub poly_right: Vec<[i32; 2]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 1928,
            height: 1208,
            poly_left: vec![[0, 550], [550, 480], [650, 950], [0, 1200]],
            poly_right: vec![[1378, 480], [1927, 550], [1927, 1200], [1278, 950]],
        }
    }
}

impl Config {
    pub fn normalize(value: &Value) -> Result<Self, Error> {
        Self::normalize_json(&numbers::json(value)?)
    }

    pub fn normalize_json(value: &JsonValue) -> Result<Self, Error> {
        if !value.is_object() {
            return Err(Error::Invalid("configuration must be a JSON object"));
        }
        let dimension = |name| {
            let value = value
                .get(name)
                .map_or(Ok(Some(0.0)), |value| numbers::python_integer(&value))?
                .ok_or(Error::Invalid("camera dimensions must be integers"))?;
            if !(1.0..=8192.0).contains(&value) {
                return Err(Error::Invalid(
                    "camera dimensions must be between 1 and 8192",
                ));
            }
            value
                .to_u32()
                .ok_or(Error::Invalid("camera dimensions must be integers"))
        };
        let width = dimension("width")?;
        let height = dimension("height")?;
        let poly_left = polygon(value.get("poly_left"), [width, height], true)?;
        let poly_right = polygon(value.get("poly_right"), [width, height], false)?;
        if poly_left.is_empty() && poly_right.is_empty() {
            return Err(Error::Invalid("annotate at least one side"));
        }
        Ok(Self {
            width,
            height,
            poly_left,
            poly_right,
        })
    }
}

fn polygon(
    value: Option<JsonValue>,
    dimensions: [u32; 2],
    left: bool,
) -> Result<Vec<[i32; 2]>, Error> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let length_error = || {
        Error::Invalid(if left {
            "poly_left must contain at most 64 points"
        } else {
            "poly_right must contain at most 64 points"
        })
    };
    let JsonView::Array(points) = value.view() else {
        return Err(length_error());
    };
    if points.len() > 64 {
        return Err(length_error());
    }
    if !points.is_empty() && points.len() < 3 {
        return Err(Error::Invalid(if left {
            "poly_left requires at least 3 points"
        } else {
            "poly_right requires at least 3 points"
        }));
    }
    points
        .iter()
        .map(|point| {
            let point_error = || {
                Error::Invalid(if left {
                    "poly_left points must be [x, y]"
                } else {
                    "poly_right points must be [x, y]"
                })
            };
            let JsonView::Array(values) = point.view() else {
                return Err(point_error());
            };
            if values.len() != 2 {
                return Err(point_error());
            }
            let mut output = [0; 2];
            for (index, value) in values.iter().enumerate() {
                let coordinate = numbers::python_number(value)?
                    .filter(|value| !value.is_nan())
                    .ok_or(Error::Invalid(if left {
                        "poly_left coordinates must be numbers"
                    } else {
                        "poly_right coordinates must be numbers"
                    }))?;
                if coordinate.is_infinite() {
                    return Err(Error::FloatOverflow);
                }
                let coordinate = coordinate.round_ties_even();
                if coordinate < 0.0 || coordinate >= f64::from(dimensions[index]) {
                    return Err(Error::Invalid(if left {
                        "poly_left point is outside the image"
                    } else {
                        "poly_right point is outside the image"
                    }));
                }
                output[index] = coordinate
                    .to_i32()
                    .ok_or(Error::Invalid("polygon coordinate overflow"))?;
            }
            Ok(output)
        })
        .collect()
}
