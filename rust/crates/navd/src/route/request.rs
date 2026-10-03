use super::{Config, Ports};
use crate::{
    geometry::Coordinate,
    json::{field, number},
    Error,
};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub point: Coordinate,
    latitude: String,
    longitude: String,
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Coordinate({}, {})",
            self.latitude, self.longitude
        )
    }
}

impl Endpoint {
    pub fn new(point: Coordinate) -> Result<Self, Error> {
        Ok(Self {
            point,
            latitude: float_text(point.latitude)?,
            longitude: float_text(point.longitude)?,
        })
    }

    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let latitude = field(value, "latitude")?;
        let longitude = field(value, "longitude")?;
        Ok(Self {
            point: Coordinate::new(
                number(latitude, "latitude")?,
                number(longitude, "longitude")?,
            ),
            latitude: number_text(latitude)?,
            longitude: number_text(longitude)?,
        })
    }

    fn pair(&self) -> String {
        format!("{},{}", self.longitude, self.latitude)
    }
}

pub fn parameter_coordinate<P: Ports>(
    ports: &mut P,
    name: &str,
) -> Result<(Option<Endpoint>, Option<Value>), Error> {
    let Some(value) = ports.parameter(name)? else {
        return Ok((None, None));
    };
    let value: Value = serde_json::from_str(&value)?;
    if value.get("latitude").is_none() || value.get("longitude").is_none() {
        return Ok((None, None));
    }
    Ok((
        Some(Endpoint::from_json(&value)?),
        value
            .get("place_name")
            .cloned()
            .filter(|value| !value.is_null()),
    ))
}

fn float_text(value: f64) -> Result<String, Error> {
    let mut text = String::new();
    openpilot_runtime_core::python_float::write_float(value, &mut text)?;
    Ok(text.replace("NaN", "nan").replace("Infinity", "inf"))
}

fn number_text(value: &Value) -> Result<String, Error> {
    match value {
        Value::Number(value) if value.is_i64() || value.is_u64() => Ok(value.to_string()),
        Value::Number(value) => float_text(value.as_f64().ok_or(Error::Field("coordinate"))?),
        Value::Bool(value) => Ok(if *value { "True" } else { "False" }.to_owned()),
        _ => Err(Error::Field("coordinate")),
    }
}

pub(super) fn route_url<P: Ports>(
    ports: &mut P,
    endpoints: (&Endpoint, &Endpoint),
    settings: (&Config, Option<f64>),
) -> Result<String, Error> {
    let (position, destination) = endpoints;
    let (config, bearing) = settings;
    let language = ports
        .parameter("LanguageSetting")?
        .map(|text| text.replace("main_", ""));
    let mut coordinates = vec![position.pair()];
    if let Some(waypoints) = ports
        .parameter("NavDestinationWaypoints")?
        .filter(|value| !value.is_empty())
    {
        let waypoints: Value = serde_json::from_str(&waypoints)?;
        for waypoint in waypoints
            .as_array()
            .ok_or(Error::Field("NavDestinationWaypoints"))?
        {
            let pair = waypoint.as_array().ok_or(Error::Field("waypoint"))?;
            if pair.len() != 2 {
                return Err(Error::Field("waypoint"));
            }
            coordinates.push(format!(
                "{},{}",
                number_text(&pair[0])?,
                number_text(&pair[1])?
            ));
        }
    }
    coordinates.push(destination.pair());
    let mut url = url::Url::parse(&format!(
        "{}/directions/v5/mapbox/driving-traffic/{}",
        config.host,
        coordinates.join(";")
    ))?;
    {
        let mut query = url.query_pairs_mut();
        if let Some(token) = &config.token {
            query.append_pair("access_token", token);
        }
        for (key, value) in [
            ("annotations", "maxspeed"),
            ("geometries", "geojson"),
            ("overview", "full"),
            ("steps", "true"),
            ("banner_instructions", "true"),
            ("alternatives", "false"),
        ] {
            query.append_pair(key, value);
        }
        if let Some(language) = language {
            query.append_pair("language", &language);
        }
        query.append_pair("waypoints", &format!("0;{}", coordinates.len() - 1));
        if let Some(bearing) = bearing {
            let bearing = (bearing + 360.).rem_euclid(360.);
            let bearing = if bearing == 0. { 0. } else { bearing };
            query.append_pair(
                "bearings",
                &format!("{bearing:.0},90{}", ";".repeat(coordinates.len() - 1)),
            );
        }
    }
    Ok(url.into())
}
