use super::{broker::Broker, camera::CameraHub, raw::RawHub, session, Command, Shutdown};
use crate::{Error, Value};
use openpilot_params::Params;
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::{
    sync::{mpsc, watch, Notify},
    task::JoinSet,
};

pub(super) async fn run(
    mut commands: mpsc::Receiver<Command>,
    mut stop: watch::Receiver<Shutdown>,
    params: Option<Params>,
    engaged: watch::Sender<bool>,
) -> Result<(), Error> {
    let mut broker = Broker::new(params, engaged);
    let raw = Rc::new(RefCell::new(RawHub::new()));
    let camera = Rc::new(RefCell::new(CameraHub::new()));
    let mut sessions = JoinSet::new();
    let mut senders = JoinSet::new();
    senders.spawn_local(CameraHub::send(Rc::clone(&camera)));
    let mut next_id = 0_u64;
    let mut wake = None;
    let registrations = Rc::new(Notify::new());
    loop {
        if *stop.borrow() == Shutdown::Force {
            break;
        }
        let delay = raw
            .borrow()
            .delay()
            .into_iter()
            .chain(camera.borrow().delay())
            .min();
        if wake.is_none() {
            wake = delay.map(|delay| tokio::time::Instant::now() + delay);
        }
        tokio::select! {
            changed=stop.changed()=>if changed.is_err() || *stop.borrow()==Shutdown::Force {break;},
            command=commands.recv()=>match command {
                Some(Command::Poll{force,reply})=>{
                    let response=match &mut broker {Ok(broker)=>broker.response(force),Err(error)=>Ok(Value::object([("ok",Value::Bool(false)),("error",Value::text(&error.to_string()))]))};
                    match reply.send(response) {Ok(())|Err(_)=>{}}
                }
                Some(Command::Launch{upgrade,spec,admission})=>{
                    next_id=next_id.wrapping_add(1);sessions.spawn_local(session::run(session::Launch{id:next_id,upgrade,spec,admission,raw:Rc::clone(&raw),camera:Rc::clone(&camera),stop:stop.clone(),registrations:Rc::clone(&registrations)}));
                }
                None=>break,
            },
            _=tokio::time::sleep_until(wake.unwrap_or_else(||tokio::time::Instant::now()+Duration::from_secs(86400))),if wake.is_some()=>{raw.borrow_mut().poll();camera.borrow_mut().poll();wake=None;},
            _=registrations.notified()=>{wake=None;},
            _=sessions.join_next(),if !sessions.is_empty()=>{},
            _=senders.join_next(),if !senders.is_empty()=>{},
        }
    }
    commands.close();
    while commands.try_recv().is_ok() {}
    drop(commands);
    sessions.abort_all();
    senders.abort_all();
    while sessions.join_next().await.is_some() {}
    while senders.join_next().await.is_some() {}
    Ok(())
}
