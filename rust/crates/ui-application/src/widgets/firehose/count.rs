use crate::context::Context;
use openpilot_logmessaged::{JsonValue, JsonView};
use openpilot_ui_framework::Error;
pub fn contribution(context: &Context, value: &JsonValue) -> Result<Option<String>, Error> {
    let proxy = match value.view() {
        JsonView::Bool(true) => 1,
        JsonView::Bool(false) => return Ok(None),
        JsonView::Integer(value) => {
            if value.starts_with('-') || value == "0" {
                return Ok(None);
            }
            if value == "1" {
                1
            } else {
                value
                    .bytes()
                    .fold(0i64, |n, d| (n * 10 + i64::from(d - b'0')) % 100)
                    + 100
            }
        }
        JsonView::Float(value) => {
            if value <= 0.0 || value.is_nan() {
                return Ok(None);
            }
            if value == 1.0 {
                1
            } else if value <= 1.0 {
                0
            } else if value % 10.0 == 1.0 && value % 100.0 != 11.0 {
                21
            } else if (2.0..=4.0).contains(&(value % 10.0))
                && !(12.0..=14.0).contains(&(value % 100.0))
            {
                2
            } else {
                5
            }
        }
        _ => {
            return Err(Error::Contract(
                "firehose count does not support numeric comparison",
            ))
        }
    };
    let text = context.trn(
        "{} segment of your driving is in the training dataset so far.",
        "{} segments of your driving is in the training dataset so far.",
        proxy,
    );
    let points = openpilot_runtime_version::python_str(value)
        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
    let number = points
        .into_iter()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or(Error::Contract("invalid firehose number text"))?;
    Ok(Some(text.replace("{}", &number)))
}
