use crate::{Calibrator, Error, Odometry, Seed, Status};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_cereal::{
    car_capnp::car_params,
    log_capnp::{camera_odometry, event, live_calibration_data},
};
use std::io::Cursor;

fn values(list: capnp::primitive_list::Reader<'_, f32>) -> Vec<f64> {
    list.iter().map(f64::from).collect()
}

pub fn odometry(message: camera_odometry::Reader<'_>) -> Result<Odometry, Error> {
    Ok(Odometry {
        trans: values(message.get_trans()?),
        rot: values(message.get_rot()?),
        trans_std: values(message.get_trans_std()?),
        wide: values(message.get_wide_from_device_euler()?),
        road: values(message.get_road_transform_trans()?),
        road_std: values(message.get_road_transform_trans_std()?),
    })
}

pub fn saved(bytes: &[u8]) -> (Seed, Option<Error>) {
    let mut seed = Seed::default();
    let result = (|| -> Result<(), Error> {
        let reader = serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
        let event = reader.get_root::<event::Reader<'_>>()?;
        let event::LiveCalibration(message) = event.which()? else {
            return Err(Error::Contract("saved Event is not liveCalibration"));
        };
        let message = message?;
        // Retain the source assignment order if a later cached field is unreadable.
        seed.rpy = values(message.get_rpy_calib()?);
        seed.valid_blocks = message.get_valid_blocks();
        seed.wide = values(message.get_wide_from_device_euler()?);
        seed.height = values(message.get_height()?);
        Ok(())
    })();
    (seed, result.err())
}

pub fn not_car(bytes: &[u8]) -> Result<bool, Error> {
    let reader = serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
    Ok(reader.get_root::<car_params::Reader<'_>>()?.get_not_car())
}

fn fill(mut list: capnp::primitive_list::Builder<'_, f32>, values: &[f64]) {
    for (index, value) in (0..list.len()).zip(values) {
        // Cereal's Float32 assignment rounds f64 and preserves NaN/infinity, including overflow.
        list.set(index, *value as f32);
    }
}

pub fn encode(calibrator: &Calibrator, timestamp: u64, valid: bool) -> Result<Vec<u8>, Error> {
    let mut builder = Builder::new_default();
    let mut event = builder.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(timestamp);
    event.set_valid(valid);
    let mut message = event.init_live_calibration();
    let rpy = if calibrator.not_car {
        vec![0.0; 3]
    } else {
        calibrator.smooth_rpy()?
    };
    let blocks = if calibrator.not_car {
        5
    } else {
        i32::from(calibrator.valid_blocks)
    };
    let percent = if calibrator.not_car {
        100
    } else {
        ((i32::from(calibrator.valid_blocks) * 100 + i32::from(calibrator.idx)) / 5).min(100)
    };
    let status = if calibrator.not_car {
        Status::Calibrated
    } else {
        calibrator.status
    };
    message.set_valid_blocks(blocks);
    message.set_cal_perc(
        i8::try_from(percent)
            .map_err(|_| Error::Contract("calibration percentage out of range"))?,
    );
    message.set_cal_status(match status {
        Status::Uncalibrated => live_calibration_data::Status::Uncalibrated,
        Status::Calibrated => live_calibration_data::Status::Calibrated,
        Status::Invalid => live_calibration_data::Status::Invalid,
        Status::Recalibrating => live_calibration_data::Status::Recalibrating,
    });
    let rpy_len = u32::try_from(rpy.len()).map_err(|_| Error::Contract("saved RPY too large"))?;
    fill(message.reborrow().init_rpy_calib(rpy_len), &rpy);
    let spread_len =
        u32::try_from(calibrator.spread.len()).map_err(|_| Error::Contract("spread too large"))?;
    fill(
        message.reborrow().init_rpy_calib_spread(spread_len),
        &calibrator.spread,
    );
    fill(
        message.reborrow().init_wide_from_device_euler(3),
        &calibrator.wide,
    );
    fill(message.init_height(1), &[calibrator.height]);
    Ok(serialize::write_message_to_words(&builder))
}
