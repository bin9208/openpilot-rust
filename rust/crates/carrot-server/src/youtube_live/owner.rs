use super::{
    clock, engine,
    engine_stream::{self, OwnedClient, Shared},
    network::Endpoint,
    service::{Command, Config, Request},
    state::State,
};
use crate::Error;
use std::{cell::RefCell, rc::Rc};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};

pub(super) async fn run(
    config: Config,
    mut commands: mpsc::Receiver<Request>,
    mut stop: watch::Receiver<bool>,
    owned: OwnedClient,
) -> Result<(), Error> {
    let endpoint = config.endpoint;
    let state = Rc::new(RefCell::new(State::new(
        (config.state_path, config.secret_path),
        config.params,
    )));
    let mut requests = JoinSet::new();
    {
        let control = Rc::clone(&state);
        let clients = std::sync::Arc::clone(&owned);
        let running = loop_service(control, clients, endpoint.clone());
        tokio::pin!(running);
        while !*stop.borrow_and_update() {
            tokio::select! {
                result = &mut running => { result?; break; }
                result = stop.changed() => { if result.is_err() { break; } }
                command = commands.recv() => match command {
                    Some(command) => dispatch(command, &state, (&endpoint, &mut requests)),
                    None => break,
                },
                completed = requests.join_next(), if !requests.is_empty() => {
                    if let Some(Err(error)) = completed { state.borrow_mut().error = error.to_string(); }
                }
            }
        }
    }
    // Stop admissions without waiting for pending send reservations on the caller's runtime.
    commands.close();
    while commands.try_recv().is_ok() {}
    drop(commands);
    requests.abort_all();
    while requests.join_next().await.is_some() {}
    engine_stream::stop(&state, &owned).await;
    engine_stream::persist(&state, true);
    Ok(())
}
async fn loop_service(state: Shared, owned: OwnedClient, endpoint: Endpoint) -> Result<(), Error> {
    loop {
        let started = std::time::Instant::now();
        if let Err(error) = engine::tick(&state, &owned, &endpoint).await {
            engine_stream::backoff(&state, &owned, (&error.to_string(), "service loop failure"))
                .await;
        }
        tokio::time::sleep(engine::delay(&state, started.elapsed())).await;
    }
}
fn dispatch(request: Request, state: &Shared, context: (&Endpoint, &mut JoinSet<()>)) {
    let Request { command, admission } = request;
    match command {
        Command::Status(reply) => {
            let _reply = reply.send(super::status::status(&mut state.borrow_mut(), clock::now()));
        }
        Command::Diagnostics(reply) => {
            let _reply = reply.send(super::diagnostics::diagnostics(
                &mut state.borrow_mut(),
                context.0,
            ));
        }
        Command::GetKey(reply) => {
            let _reply = reply.send(state.borrow_mut().keys.get().value());
        }
        Command::SetKey(key, reply) => {
            let result = state.borrow_mut().keys.set(&key);
            if result.is_ok() {
                state.borrow_mut().error.clear();
                engine_stream::persist(state, true);
            }
            let _reply = reply.send(
                result.map(|()| super::status::status(&mut state.borrow_mut(), clock::now())),
            );
        }
        Command::ClearKey(reply) => {
            let result = state.borrow_mut().keys.clear();
            if result.is_ok() {
                engine_stream::persist(state, true);
            }
            let _reply = reply.send(
                result.map(|()| super::status::status(&mut state.borrow_mut(), clock::now())),
            );
        }
        Command::Verify(value, reply) => {
            let state = Rc::clone(state);
            let endpoint = context.0.clone();
            context.1.spawn_local(async move {
                let key = match value {
                    Some(value) if !matches!(value, crate::Value::Null) => {
                        super::key::Key::extract(&value).unwrap_or_default()
                    }
                    Some(crate::Value::Null) | None => state.borrow_mut().keys.get(),
                    Some(_) => super::key::Key::default(),
                };
                let (format_ok, format_message) = key.validate();
                let reachability =
                    tokio::task::spawn_blocking(move || (endpoint.verify(), admission)).await;
                let (reachable, _guard) = match reachability {
                    Ok((reachable, guard)) => (reachable, Some(guard)),
                    Err(error) => (
                        (false, format!("YouTube RTMPS ingest unreachable: {error}")),
                        None,
                    ),
                };
                let _reply = reply.send(super::diagnostics::validation(
                    &mut state.borrow_mut(),
                    &key,
                    ((format_ok, format_message), reachable),
                ));
            });
        }
        Command::Test(reply) => {
            let state = Rc::clone(state);
            let endpoint = context.0.clone();
            context.1.spawn_local(async move {
                let key = state.borrow_mut().keys.get();
                let reachability =
                    tokio::task::spawn_blocking(move || (endpoint.verify(), admission)).await;
                let (reachable, _guard) = match reachability {
                    Ok((reachable, guard)) => (reachable, Some(guard)),
                    Err(error) => (
                        (false, format!("YouTube RTMPS ingest unreachable: {error}")),
                        None,
                    ),
                };
                let _reply = reply.send(super::diagnostics::test(
                    &mut state.borrow_mut(),
                    (key, reachable),
                ));
            });
        }
    }
}
