use crate::Error;
use openpilot_runtime_core::python_float::{parse, write_float};

pub struct Destination {
    latitude: f64,
    longitude: f64,
    pub default_waypoint: bool,
}

impl Destination {
    pub fn from_argument(argument: Option<&str>) -> Result<Self, Error> {
        let Some(argument) = argument else {
            return Ok(Self {
                latitude: 32.71160109904473,
                longitude: -117.12556569985693,
                default_waypoint: true,
            });
        };
        let tail = argument
            .rsplit("/@")
            .next()
            .ok_or(Error::Field("destination URL"))?;
        let coordinate = tail
            .split('/')
            .next()
            .ok_or(Error::Field("destination URL"))?;
        let mut parts = coordinate.split(',');
        let latitude = parts
            .next()
            .and_then(parse)
            .ok_or(Error::Field("destination latitude"))?;
        let longitude = parts
            .next()
            .and_then(parse)
            .ok_or(Error::Field("destination longitude"))?;
        Ok(Self {
            latitude,
            longitude,
            default_waypoint: false,
        })
    }

    pub fn parameter(&self) -> Result<String, Error> {
        let mut output = "{\"latitude\": ".to_owned();
        write_float(self.latitude, &mut output)?;
        output.push_str(", \"longitude\": ");
        write_float(self.longitude, &mut output)?;
        output.push('}');
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_marker_and_extra_fields_preserve_source_selection() {
        let value =
            Destination::from_argument(Some("https://maps/x/@0,1/a/@37.0,127.0,15z/data")).unwrap();
        assert_eq!(
            value.parameter().unwrap(),
            "{\"latitude\": 37.0, \"longitude\": 127.0}"
        );
        assert!(!value.default_waypoint);
    }

    #[test]
    fn source_float_syntax_and_signed_zero_are_retained() {
        let value = Destination::from_argument(Some("\u{2003}３_７.０,-0.0")).unwrap();
        assert_eq!(
            value.parameter().unwrap(),
            "{\"latitude\": 37.0, \"longitude\": -0.0}"
        );
        assert!(Destination::from_argument(Some("37.0,not-a-longitude")).is_err());
    }
}
