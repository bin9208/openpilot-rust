use super::{
    clock,
    engine_stream::{self, OwnedClient, Shared},
    network::Endpoint,
    profiles,
    samples::Warmup,
};
use crate::Error;
pub(super) async fn ready(
    state: &Shared,
    owned: &OwnedClient,
    endpoint: &Endpoint,
) -> Result<Option<(super::key::Key, profiles::Profile)>, Error> {
    let now = clock::now();
    if !state.borrow_mut().enabled(now.mono) {
        engine_stream::stop(state, owned).await;
        {
            let mut state = state.borrow_mut();
            state.warmup = Warmup::default();
            state.error.clear();
            state.failures = 0;
            state.next_retry = 0.0;
        }
        engine_stream::set_state(state, "disabled");
        return Ok(None);
    }
    let key = state.borrow_mut().keys.get();
    if !key.configured() {
        engine_stream::stop(state, owned).await;
        state.borrow_mut().warmup = Warmup::default();
        engine_stream::set_state(state, "needs_setup");
        return Ok(None);
    }
    let unavailable = {
        let state = state.borrow();
        if !state.capabilities.transport.get("available").truth() {
            Some(
                state
                    .capabilities
                    .transport
                    .get("error")
                    .string()
                    .unwrap_or_else(|_| "librtmp is unavailable".into()),
            )
        } else if !state.capabilities.ready {
            Some("FFmpeg FLV/AAC muxer is unavailable".into())
        } else {
            None
        }
    };
    if let Some(error) = unavailable {
        engine_stream::stop(state, owned).await;
        {
            let mut state = state.borrow_mut();
            state.warmup = Warmup::default();
            state.error = error;
        }
        engine_stream::set_state(state, "error");
        return Ok(None);
    }
    let retry_pending = {
        let state = state.borrow();
        state.writer.is_none() && state.next_retry != 0.0 && now.mono < state.next_retry
    };
    if retry_pending {
        engine_stream::set_state(state, "backoff");
        return Ok(None);
    }
    refresh_network(state, endpoint).await?;
    let profile = state.borrow_mut().profile(clock::now().mono);
    let changed = {
        let state = state.borrow();
        state.writer.is_some() && state.active_quality != profile.quality
    };
    if changed {
        state.borrow_mut().log(
            &format!("video mode changed to {}; restarting", profile.label),
            "info",
        );
        engine_stream::stop(state, owned).await;
        state.borrow_mut().warmup = Warmup::default();
    }
    let writer_error = state
        .borrow()
        .writer
        .as_ref()
        .and_then(|writer| writer.snapshot().ok())
        .map(|stats| stats.error)
        .filter(|error| !error.is_empty());
    if let Some(error) = writer_error {
        engine_stream::backoff(
            state,
            owned,
            (
                &format!("YouTube RTMPS publish failed: {error}"),
                "RTMPS publish failed",
            ),
        )
        .await;
        return Ok(None);
    }
    let connected = {
        let mut state = state.borrow_mut();
        let now = clock::now().mono;
        if state.writer.is_some() && now - state.conn_checked >= 1.0 {
            state.conn_checked = now;
            state
                .writer
                .as_ref()
                .map(|writer| writer.client.try_connected())
                .transpose()?
                .flatten()
        } else {
            None
        }
    };
    if connected == Some(false) {
        engine_stream::backoff(
            state,
            owned,
            ("YouTube RTMPS connection closed", "RTMPS connection closed"),
        )
        .await;
        return Ok(None);
    }
    Ok(Some((key, profile)))
}

async fn refresh_network(state: &Shared, endpoint: &Endpoint) -> Result<(), Error> {
    let check = {
        let mut state = state.borrow_mut();
        let now = clock::now().mono;
        if state.writer.is_some()
            || (state.net_checked != 0.0
                && now - state.net_checked < if state.net_ok { 8.0 } else { 2.0 })
        {
            false
        } else {
            state.net_checked = now;
            true
        }
    };
    if !check {
        return Ok(());
    }
    let endpoint = endpoint.clone();
    let reachable = tokio::task::spawn_blocking(move || endpoint.reachable())
        .await
        .map_err(|error| Error::Source(error.to_string()))?;
    let mut state = state.borrow_mut();
    if reachable != state.net_ok {
        state.log(
            if reachable {
                "network reachable"
            } else {
                "network unreachable"
            },
            if reachable { "info" } else { "warn" },
        );
    }
    state.net_ok = reachable;
    Ok(())
}
