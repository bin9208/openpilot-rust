use super::{
    admission::Counter,
    context::{Context, Event},
    local, network, session, Command, Config,
};
use crate::{tools::runner::Runner, web_sound::Shutdown, Error};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};

pub(super) async fn run(
    mut commands: mpsc::UnboundedReceiver<Command>,
    mut stop: watch::Receiver<Shutdown>,
    config: Config,
    count: Arc<Counter>,
) -> Result<(), Error> {
    let (events, mut received) = mpsc::unbounded_channel();
    let (cancel, cancelled) = watch::channel(false);
    let context = Rc::new(Context {
        gate: tokio::sync::Mutex::new(()),
        current: RefCell::new(None),
        binary: super::tunnel_binary::Config::original(&config.data),
        runner: Runner {
            repository: config.repository.clone(),
            launcher: config.launcher.clone(),
            lock: None,
            stopped: cancelled,
        },
        http: network::client(config.shared_http)?,
        download: network::download()?,
        config,
        stop: stop.clone(),
        events,
        count,
    });
    let mut tasks = JoinSet::new();
    let mut next = 0_u64;
    loop {
        if *stop.borrow() == Shutdown::Force {
            break;
        }
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() == Shutdown::Force { break; }
            }
            command = commands.recv() => match command {
                Some(Command::Quiesce(admission)) => {
                    let context = Rc::clone(&context);
                    tasks.spawn_local(async move { let _admission = admission; context.stop_session("shutdown").await.map(|_| ()) });
                }
                Some(Command::Snapshot(reply)) => { let _accepted = reply.send(context.snapshot()); }
                Some(Command::Start { options, admission, reply }) => {
                    let context = Rc::clone(&context);
                    tasks.spawn_local(async move { let _admission = admission; let _accepted = reply.send(context.start_session(options).await); Ok(()) });
                }
                Some(Command::Stop { admission, reply }) => {
                    let context = Rc::clone(&context);
                    tasks.spawn_local(async move { let _admission = admission; let _accepted = reply.send(context.stop_session("user").await); Ok(()) });
                }
                Some(Command::Decision { id, approve, admission, reply }) => {
                    let context = Rc::clone(&context);
                    tasks.spawn_local(async move { let _admission = admission; let _accepted = reply.send(Ok(context.decision(&id, approve).await)); Ok(()) });
                }
                Some(Command::Launch { upgrade, mode, admission }) => {
                    next = next.wrapping_add(1);
                    let context = Rc::clone(&context);
                    let id = next;
                    tasks.spawn_local(async move { let _admission = admission; session::run(context, upgrade, mode, id).await });
                }
                None => break,
            },
            event = received.recv() => if let Some(queued) = event {
                let context = Rc::clone(&context);
                next = next.wrapping_add(1);
                let id = next;
                tasks.spawn_local(async move {
                    let _admission = queued.admission;
                    match queued.event {
                        Event::Cleanup(session) => context.cleanup(session).await,
                        Event::Expiry(session) => context.expire_session(session).await,
                        Event::CommandExpiry(session, id) => context.expire_command(session, id).await,
                        Event::Local(listener, session, stop) => local::run(context, listener, session, stop).await,
                        Event::Socket(upgrade, mode, admission) => { let _admission = admission; session::run(context, upgrade, mode, id).await },
                    }
                });
            },
            joined = tasks.join_next(), if !tasks.is_empty() => match joined {
                Some(Ok(Err(error))) => eprintln!("Support operation: {error}"),
                Some(Err(error)) if !error.is_cancelled() => eprintln!("Support task: {error}"),
                Some(Ok(Ok(())) | Err(_)) | None => {},
            }
        }
    }
    cancel.send_replace(true);
    commands.close();
    while commands.try_recv().is_ok() {}
    drop(commands);
    received.close();
    while received.try_recv().is_ok() {}
    drop(received);
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    context.current.borrow_mut().take();
    Ok(())
}
