use super::{
    bridge::Bridge,
    runtime::{Command, Spec},
    session,
};
use crate::{web_sound::Shutdown, Error};
use openpilot_params::Params;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, watch, Notify},
    task::JoinSet,
};

pub(super) async fn run(
    mut commands: mpsc::Receiver<Command>,
    mut stop: watch::Receiver<Shutdown>,
    params: Option<Params>,
) -> Result<(), Error> {
    let wake = Rc::new(Notify::new());
    let bridge = Rc::new(RefCell::new(Bridge::new(params, Rc::clone(&wake))));
    let mut sessions = JoinSet::new();
    let mut next_id = 0_u64;
    let mut poll_at = None;
    loop {
        if *stop.borrow() == Shutdown::Force {
            break;
        }
        if poll_at.is_none() && bridge.borrow().running {
            poll_at = Some(tokio::time::Instant::now());
        }
        tokio::select! {
            changed=stop.changed()=>if changed.is_err() || *stop.borrow()==Shutdown::Force {break;},
            command=commands.recv()=>match command {
                Some(Command::Allowed(reply))=>{let allowed=bridge.borrow_mut().allowed(true);match reply.send(allowed){Ok(())|Err(_)=>{}}},
                Some(Command::Status(reply))=>{let status=bridge.borrow_mut().status();match reply.send(status){Ok(())|Err(_)=>{}}},
                Some(Command::Diagnostic{peer,value,reply})=>{
                    bridge.borrow_mut().diagnostics.insert(peer.chars().take(128).collect(),(Instant::now(),value));match reply.send(()){Ok(())|Err(_)=>{}}
                },
                Some(Command::Launch{upgrade,spec:Spec{identity,mode},admission})=>{
                    next_id=next_id.wrapping_add(1);
                    sessions.spawn_local(session::run(session::Launch{id:next_id,upgrade,identity,mode,admission,bridge:Rc::clone(&bridge),stop:stop.clone()}));
                },
                None=>break,
            },
            _=tokio::time::sleep_until(poll_at.unwrap_or_else(||tokio::time::Instant::now()+Duration::from_secs(86400))),if poll_at.is_some()=>{
                let delay=bridge.borrow_mut().poll();
                poll_at=bridge.borrow().running.then(||tokio::time::Instant::now()+delay);
            },
            _=wake.notified()=>{poll_at=None;},
            _=sessions.join_next(),if !sessions.is_empty()=>{},
        }
    }
    commands.close();
    while commands.try_recv().is_ok() {}
    drop(commands);
    sessions.abort_all();
    while sessions.join_next().await.is_some() {}
    bridge.borrow_mut().stop();
    Ok(())
}
