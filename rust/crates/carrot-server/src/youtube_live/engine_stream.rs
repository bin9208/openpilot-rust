use super::{
    clock,
    network::Endpoint,
    profiles,
    rtmp::Client,
    samples::Samples,
    state::State,
    writer::{Codec, Writer},
};
use crate::Error;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

pub(super) type Shared = Rc<RefCell<State>>;
pub(super) type OwnedClient = Arc<Mutex<Option<Arc<Client>>>>;
pub(super) fn persist(state: &Shared, force: bool) {
    let now = clock::now();
    let mut state = state.borrow_mut();
    if !force && now.mono - state.status_written < 5.0 {
        return;
    }
    state.status_written = now.mono;
    let mut value = super::status::status(&mut state, now);
    if crate::json_fields::set(&mut value, "updated_at", crate::Value::Float(now.wall)).is_ok() {
        let _persisted = super::keys::write(&state.state_path, &value, None);
    }
}
pub(super) fn set_state(state: &Shared, name: &'static str) {
    state.borrow_mut().state = name;
    persist(state, false);
}
pub(super) async fn stop(state: &Shared, owned: &OwnedClient) {
    let (writer, start_bytes) = {
        let mut state = state.borrow_mut();
        state.connected = false;
        (state.writer.take(), state.session_start_bytes)
    };
    if let Some(mut writer) = writer {
        set_state(state, "stopping");
        let discarded = writer.stop().await.unwrap_or(0);
        let mut state = state.borrow_mut();
        state.discarded = state
            .discarded
            .saturating_add(u64::try_from(discarded).unwrap_or(u64::MAX));
        state.bytes_sent = start_bytes.saturating_add(i128::from(writer.client.bytes_written()));
    }
    state.borrow_mut().started = clock::Stamp {
        mono: 0.0,
        wall: 0.0,
    };
    if let Ok(mut client) = owned.lock() {
        client.take();
    }
}
pub(super) async fn backoff(state: &Shared, owned: &OwnedClient, reason: (&str, &str)) {
    {
        let mut state = state.borrow_mut();
        state.connected = false;
        state.error = reason.0.into();
        state.log(reason.0, "warn");
    }
    stop(state, owned).await;
    state
        .borrow_mut()
        .schedule_backoff(reason.1, clock::now().mono);
    persist(state, false);
}
pub(super) async fn start(
    state: &Shared,
    owned: &OwnedClient,
    setup: (Endpoint, super::key::Key, Vec<u8>),
) -> Result<(), Error> {
    stop(state, owned).await;
    {
        let mut state = state.borrow_mut();
        state.error.clear();
        state.restart_count = state.restart_count.saturating_add(1);
        state.last_start = clock::now().mono;
        state.state = "starting";
    }
    persist(state, false);
    state
        .borrow_mut()
        .log("connecting to YouTube RTMPS", "info");
    let client = Arc::new(Client::new(setup.1.url(&setup.0.base)?)?);
    if let Ok(mut active) = owned.lock() {
        *active = Some(Arc::clone(&client));
    }
    let mut writer = Writer::spawn(
        client,
        Codec {
            header: setup.2,
            fps: profiles::FPS,
        },
    )?;
    let response = writer.initialization()?;
    state.borrow_mut().writer = Some(writer);
    let result = response
        .await
        .map_err(|_| Error::Source("RTMP writer exited during startup".into()))?;
    result.map_err(Error::Source)?;
    let now = clock::now();
    let mut state = state.borrow_mut();
    state.connected = true;
    state.started = now;
    state.session_start_bytes = state.bytes_sent;
    state.next_retry = 0.0;
    state.conn_checked = now.mono;
    state.active_quality = state.profile(now.mono).quality;
    state.captions.reset();
    state.samples = Samples::default();
    let (width, height) = (state.width, state.height);
    state.log(&format!("stream started ({width}x{height})"), "info");
    Ok(())
}
pub(super) async fn write(
    state: &Shared,
    owned: &OwnedClient,
    frame: super::writer_state::Frame,
) -> Result<bool, Error> {
    let rejection = {
        let mut state = state.borrow_mut();
        let Some(writer) = state.writer.as_ref() else {
            return Ok(false);
        };
        if writer.enqueue(frame)? {
            let stats = writer.snapshot()?;
            state.bytes_sent = state
                .session_start_bytes
                .saturating_add(i128::from(writer.client.bytes_written()));
            if state.started.mono != 0.0
                && clock::now().mono - state.started.mono >= 10.0
                && stats.frames_written > 0
                && stats.pending_frames <= 15
                && stats.pending_bytes <= 2 * 1024 * 1024
            {
                state.failures = 0;
            }
            None
        } else {
            let reason = writer.snapshot()?.rejection;
            if reason.contains("backlog") {
                state.backlog_restarts = state.backlog_restarts.saturating_add(1);
            }
            Some(if reason.is_empty() {
                "RTMP writer rejected a frame".into()
            } else {
                reason
            })
        }
    };
    if let Some(rejection) = rejection {
        backoff(
            state,
            owned,
            (
                &format!("YouTube RTMPS publish failed: {rejection}"),
                "RTMPS publish failed",
            ),
        )
        .await;
        return Ok(false);
    }
    let starved = {
        let mut state = state.borrow_mut();
        let profile = state.profile(clock::now().mono);
        let stream = (state.started.mono, state.bytes_sent);
        state.samples.starved(stream, profile, clock::now().mono)
    };
    if let Some(error) = starved {
        backoff(state, owned, (&error, "upload starved")).await;
        return Ok(false);
    }
    set_state(state, "live");
    Ok(true)
}
