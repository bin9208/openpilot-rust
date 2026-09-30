use crate::{publication::Messages, Error};
use openpilot_cereal::log_capnp::{event, jetlink_frame_status};
use openpilot_jetlink::{
    runtime::Status,
    transition::{Phase, Source},
};
use openpilot_params::Params;
use std::io::Cursor;
pub fn raw_predictions(
    requested: bool,
    source: Source,
    native: &[u8],
) -> Result<Option<&[u8]>, Error> {
    if !requested {
        return Ok(None);
    }
    match source {
        Source::Native => Ok(Some(native)),
        Source::Jetlink => Err(Error::JetlinkRawPredictionsUnavailable),
    }
}
pub fn persist(
    params: &Params,
    stored: &mut Option<(bool, bool)>,
    status: &Status,
) -> Result<(), Error> {
    let state = (
        status.decision.source == Source::Jetlink,
        status.decision.loss_latched,
    );
    if *stored != Some(state) {
        params.put_bool("JetlinkActive", state.0)?;
        params.put_bool("JetlinkLossLatched", state.1)?;
        *stored = Some(state);
    }
    Ok(())
}
fn fill(mut builder: jetlink_frame_status::Builder<'_>, status: &Status) {
    builder.set_source(match status.decision.source {
        Source::Native => jetlink_frame_status::Source::Native,
        Source::Jetlink => jetlink_frame_status::Source::Jetlink,
    });
    builder.set_phase(match status.decision.phase {
        Phase::Off => "OFF",
        Phase::Preparing => "PREPARING",
        Phase::Ready => "READY",
        Phase::Active => "ACTIVE",
        Phase::Lost => "LOST",
    });
    builder.set_loss_latched(status.decision.loss_latched);
    builder.set_generation(&status.generation);
    builder.set_frame_id(status.frame);
    builder.set_execution_ms(status.execution_ms as f32);
    builder.set_validated(status.validated);
}
pub fn publication(messages: &mut Messages, status: &Status, fresh: bool) -> Result<(), Error> {
    for bytes in [
        &mut messages.model,
        &mut messages.driving,
        &mut messages.pose,
    ] {
        let reader = capnp::serialize::read_message(
            Cursor::new(&*bytes),
            capnp::message::ReaderOptions::new(),
        )?;
        let mut message = capnp::message::Builder::new_default();
        message.set_root(reader.get_root::<event::Reader>()?)?;
        let mut root = message.get_root::<event::Builder>()?;
        if !fresh {
            root.set_valid(false);
        }
        match root.which()? {
            event::ModelV2(value) => fill(value?.init_jetlink(), status),
            event::DrivingModelData(value) => fill(value?.init_jetlink(), status),
            event::CameraOdometry(_) => {}
            _ => return Err(Error::Contract("Jetlink publication topic")),
        }
        *bytes = capnp::serialize::write_message_to_words(&message);
    }
    Ok(())
}
