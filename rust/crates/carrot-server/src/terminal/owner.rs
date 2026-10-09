use super::{
    pty, session,
    state::{Attachment, Shell, State},
    tmux, Command, Config,
};
use crate::{web_sound::Shutdown, Error, Value};
use std::{future::pending, sync::Arc};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};

pub(super) async fn run(
    mut commands: mpsc::UnboundedReceiver<Command>,
    sender: mpsc::UnboundedSender<Command>,
    mut stop: watch::Receiver<Shutdown>,
    config: Config,
) -> Result<(), Error> {
    let (cancel, cancelled) = watch::channel(false);
    let runner = tmux::runner(&config, cancelled)?;
    let mut state = State::new();
    let mut sessions = JoinSet::new();
    let mut writes = JoinSet::new();
    let mut captures = JoinSet::new();
    let mut next = 0_u64;
    loop {
        if *stop.borrow() == Shutdown::Force {
            break;
        }
        let fd = state
            .generation
            .as_ref()
            .map(|generation| Arc::clone(&generation.master));
        tokio::select! {
            changed = stop.changed() => if changed.is_err() || *stop.borrow() == Shutdown::Force { break; },
            command = commands.recv() => match command {
                Some(Command::Capture { argv, admission, reply }) => {
                    let runner = runner.clone();
                    let cwd = config.cli_cwd.clone();
                    captures.spawn_local(async move {
                        let _admission = admission;
                        let result = runner.raw_capture(crate::tools::runner::Command { argv: &argv, cwd: Some(&cwd), timeout: Some(std::time::Duration::from_secs(45)) }).await;
                        let _accepted = reply.send(result);
                    });
                }
                Some(Command::Launch { upgrade, spec, admission }) => {
                    next = next.wrapping_add(1);
                    sessions.spawn_local(session::run(
                        session::Start { upgrade, spec, admission, id: next },
                        session::Owner { sender: sender.clone(), stop: stop.clone(), config: config.clone(), runner: runner.clone() },
                    ));
                }
                Some(Command::Snapshot(reply)) => { let _result = reply.send(state.snapshot()); }
                Some(Command::Attach { id, client, eligible, reset, reply }) => { let _result = reply.send(state.attach(Shell { config: &config, runner: &runner }, Attachment { id, client, eligible, reset }, &mut stop).await); }
                Some(Command::Detach(id)) => state.detach(id),
                Some(Command::Write { bytes, reply }) => {
                    if let Some(generation) = &mut state.generation {
                        if generation.alive()? {
                            let fd = Arc::clone(&generation.master);
                            writes.spawn_local(async move { let _result = reply.send(pty::write(fd, &bytes).await); });
                        } else { let _result = reply.send(Err(Error::Source("terminal session is not running".into()))); }
                    } else { let _result = reply.send(Err(Error::Source("terminal session is not running".into()))); }
                }
                Some(Command::Resize { rows, reply }) => { let _result = reply.send(state.resize(rows, &mut stop).await); }
                Some(Command::Clear(reply)) => { state.history.clear(); let _result = reply.send(Ok(())); }
                None => break,
            },
            bytes = async { match fd { Some(fd) => pty::read(fd).await, None => pending().await } } => {
                match bytes {
                    Ok(bytes) if !bytes.is_empty() => state.chunk(&bytes, &mut stop).await,
                    Ok(_) | Err(_) => {
                        let code = state.generation.as_mut().map(|generation| generation.exit_code()).transpose()?.flatten().map_or(Value::Null, Value::integer);
                        state.finish(code, &mut stop).await;
                    }
                }
            }
            joined = sessions.join_next(), if !sessions.is_empty() => {
                if let Some(Ok(Err(error))) = joined { eprintln!("Terminal session: {error}"); }
            }
            _ = writes.join_next(), if !writes.is_empty() => {}
            _ = captures.join_next(), if !captures.is_empty() => {}
        }
    }
    cancel.send_replace(true);
    commands.close();
    while commands.try_recv().is_ok() {}
    drop(commands);
    sessions.abort_all();
    writes.abort_all();
    captures.abort_all();
    state.generation.take();
    while sessions.join_next().await.is_some() {}
    while writes.join_next().await.is_some() {}
    while captures.join_next().await.is_some() {}
    Ok(())
}
