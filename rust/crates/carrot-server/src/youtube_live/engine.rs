use super::{
    clock,
    engine_stream::{self, OwnedClient, Shared},
    input::Frame,
    network::Endpoint,
    profiles,
    samples::Warmup,
    writer_state,
};
use crate::Error;
use std::time::Duration;

pub(super) async fn tick(
    state: &Shared,
    owned: &OwnedClient,
    endpoint: &Endpoint,
) -> Result<(), Error> {
    let Some((key, profile)) = super::engine_preflight::ready(state, owned, endpoint).await? else {
        return Ok(());
    };
    let (frame, warmup_ready) = receive(state, profile);
    let Some(frame) = frame else {
        let (running, age) = {
            let state = state.borrow();
            (
                state.writer.is_some(),
                (state.last_frame.mono != 0.0).then_some(clock::now().mono - state.last_frame.mono),
            )
        };
        if running && age.is_some_and(|age| age > 8.0) {
            engine_stream::backoff(
                state,
                owned,
                (&format!("no {} frames", profiles::SOURCE), "frame timeout"),
            )
            .await;
            state.borrow_mut().warmup = Warmup::default();
            return Ok(());
        }
        if !running && age.is_some_and(|age| age > 2.0) {
            state.borrow_mut().warmup = Warmup::default();
        }
        engine_stream::set_state(state, if running { "live" } else { "waiting_frame" });
        return Ok(());
    };
    {
        let mut state = state.borrow_mut();
        state.last_frame = clock::now();
        state.frame_id = frame.id;
        state.width = frame.width;
        state.height = frame.height;
    }
    if state.borrow().writer.is_none() {
        if !warmup_ready {
            engine_stream::set_state(state, "waiting_stable_source");
            return Ok(());
        }
        if !frame.keyframe || frame.header.is_empty() {
            engine_stream::set_state(state, "waiting_keyframe");
            return Ok(());
        }
        if let Err(error) = super::h264::validate_start(&frame.header, &frame.data) {
            state.borrow_mut().error = format!("H.264 stream start rejected: {error}");
            engine_stream::set_state(state, "waiting_keyframe");
            return Ok(());
        }
        if !state.borrow().net_ok {
            engine_stream::set_state(state, "waiting_network");
            return Ok(());
        }
        let throttled = {
            let state = state.borrow();
            state.last_start != 0.0 && clock::now().mono - state.last_start < 3.0
        };
        if throttled {
            {
                let mut state = state.borrow_mut();
                state.next_retry = state.next_retry.max(state.last_start + 3.0);
            }
            engine_stream::set_state(state, "backoff");
            return Ok(());
        }
        if let Err(error) =
            engine_stream::start(state, owned, (endpoint.clone(), key, frame.header)).await
        {
            engine_stream::backoff(
                state,
                owned,
                (
                    &format!("YouTube RTMPS start failed: {error}"),
                    "RTMPS start failed",
                ),
            )
            .await;
            return Ok(());
        }
    }
    let data = {
        let mut state = state.borrow_mut();
        let caption = state
            .settings
            .boolean("CarrotYouTubeTimestamp", clock::now().mono);
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let data = state.captions.inject(&frame.data, caption, &stamp)?;
        state
            .samples
            .record(clock::now().mono, data.len(), frame.keyframe);
        data
    };
    engine_stream::write(
        state,
        owned,
        writer_state::Frame {
            payload: data,
            keyframe: frame.keyframe,
        },
    )
    .await?;
    Ok(())
}
fn receive(state: &Shared, profile: profiles::Profile) -> (Option<Frame>, bool) {
    let mut state = state.borrow_mut();
    let mut latest = None;
    let mut warmup_ready = false;
    loop {
        match state.input.receive(profile.quality) {
            Ok(Some(frame)) if !frame.data.is_empty() => {
                if state.writer.is_none() {
                    warmup_ready = state.warmup.observe(&frame, profile, clock::now().mono);
                }
                latest = Some(frame);
                if state.writer.is_some() {
                    break;
                }
            }
            Ok(Some(_)) | Ok(None) => break,
            Err(error) => {
                state.error = format!("{} recv failed: {error}", profiles::SOURCE);
                break;
            }
        }
    }
    (latest, warmup_ready)
}
pub(super) fn delay(state: &Shared, elapsed: Duration) -> Duration {
    let period = Duration::from_millis(if state.borrow().writer.is_some() {
        10
    } else {
        250
    });
    period.saturating_sub(elapsed)
}
