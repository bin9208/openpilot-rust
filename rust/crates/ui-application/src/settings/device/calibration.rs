use crate::{context::Context, params::Read, Error};
use openpilot_cereal::log_capnp::{event, live_calibration_data::Status};
fn with_event<T>(
    context: &Context,
    key: &str,
    read: impl FnOnce(event::Reader<'_>) -> Result<T, Error>,
) -> Result<Option<T>, Error> {
    let Some(bytes) = context.params.bytes(key)?.filter(|bytes| !bytes.is_empty()) else {
        return Ok(None);
    };
    let result = (|| -> Result<T, Error> {
        let reader =
            capnp::serialize::read_message(&mut std::io::Cursor::new(bytes), Default::default())?;
        read(reader.get_root::<event::Reader<'_>>()?)
    })();
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            openpilot_startup_ui::logging::emit(
                openpilot_logging::record::Level::Error,
                format!("invalid {key}: {error}"),
            );
            Ok(None)
        }
    }
}
fn angles(context: &Context) -> Result<Option<(f64, f64)>, Error> {
    Ok(with_event(context, "CalibrationParams", |event| {
        let event::Which::LiveCalibration(calib) = event.which()? else {
            return Err(Error::Contract("CalibrationParams event type"));
        };
        let calib = calib?;
        if calib.get_cal_status()? == Status::Uncalibrated {
            return Ok(None);
        }
        let rpy = calib.get_rpy_calib()?;
        if rpy.len() < 3 {
            return Err(Error::Contract("calibration angles missing"));
        }
        Ok(Some((
            f64::from(rpy.get(1)).to_degrees(),
            f64::from(rpy.get(2)).to_degrees(),
        )))
    })?
    .flatten())
}
pub(super) fn write_position(context: &Context) -> Result<(), Error> {
    if let Some((pitch, yaw)) = angles(context)? {
        context.params.put(
            "DevicePosition",
            format!(
                "{:.1}° {} {:.1}° {}",
                pitch.abs(),
                if pitch > 0.0 { "v" } else { "^" },
                yaw.abs(),
                if yaw > 0.0 { "<" } else { ">" }
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}
pub(super) fn description(context: &Context) -> Result<String, Error> {
    let mut desc = context.tr(super::build::CALIBRATION);
    if let Some((pitch, yaw)) = angles(context)? {
        let values = [
            format!("{:.1}", pitch.abs()),
            context.tr(if pitch > 0.0 { "down" } else { "up" }),
            format!("{:.1}", yaw.abs()),
            context.tr(if yaw > 0.0 { "left" } else { "right" }),
        ];
        let mut text = context
            .tr(" Your device is pointed {:.1f}° {} and {:.1f}° {}.")
            .replace("{:.1f}", "{}");
        for value in values {
            text = text.replacen("{}", &value, 1);
        }
        desc.push_str(&text);
    }
    let lag = with_event(context, "LiveDelay", |event| {
        let event::Which::LiveDelay(delay) = event.which()? else {
            return Err(Error::Contract("LiveDelay event type"));
        };
        Ok(delay?.get_cal_perc())
    })?
    .unwrap_or(0);
    desc.push_str(&if lag < 100 {
        context
            .tr("<br><br>Steering lag calibration is {}% complete.")
            .replacen("{}", &lag.to_string(), 1)
    } else {
        context.tr("<br><br>Steering lag calibration is complete.")
    });
    let torque = with_event(context, "LiveTorqueParameters", |event| {
        let event::Which::LiveTorqueParameters(torque) = event.which()? else {
            return Err(Error::Contract("LiveTorqueParameters event type"));
        };
        let torque = torque?;
        Ok((torque.get_use_params(), torque.get_cal_perc()))
    })?;
    if let Some((true, percentage)) = torque {
        desc.push_str(&if percentage < 100 {
            context
                .tr(" Steering torque response calibration is {}% complete.")
                .replacen("{}", &percentage.to_string(), 1)
        } else {
            context.tr(" Steering torque response calibration is complete.")
        });
    }
    desc.push_str("<br><br>");
    desc.push_str(&context.tr("openpilot is continuously calibrating, resetting is rarely required. Resetting calibration will restart openpilot if the car is powered on."));
    Ok(desc)
}
