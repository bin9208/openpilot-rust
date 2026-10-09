use super::http::{internal, reply, Reply};
use super::{Application, Profile, SessionHandle};
use crate::{
    network::Network,
    request::StreamRequest,
    session::{Publishers, Session},
    Error,
};
use hyper::StatusCode;
use openpilot_params::Params;
use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

impl Application {
    pub(super) fn new(profile: Profile, network: Network) -> Result<Self, Error> {
        let app = Self {
            carrot: profile.carrot,
            debug: profile.debug,
            network,
            streams: RefCell::new(Vec::new()),
            stream_lock: Mutex::new(()),
            publishers: RefCell::new(Publishers::default()),
            params: if profile.carrot {
                Some(Params::for_runtime()?)
            } else {
                None
            },
            active: Cell::new(None),
            shutting_down: Cell::new(false),
        };
        app.set_active(false);
        Ok(app)
    }

    fn set_active(&self, active: bool) {
        let Some(params) = &self.params else { return };
        if self.active.replace(Some(active)) != Some(active) {
            if let Err(error) = params.put_bool("CarrotVisionActive", active) {
                eprintln!("WebRTC Params update failed: {error}");
            }
        }
    }

    fn sync_active(&self) {
        let active = self.streams.borrow().iter().any(|session| session.road);
        self.set_active(active);
    }

    fn remove(&self, identifier: &str) {
        self.streams
            .borrow_mut()
            .retain(|session| session.identifier != identifier);
        self.sync_active();
    }

    async fn prune(&self) {
        let streams = self.streams.borrow().clone();
        for session in streams {
            let mut session = session.value.lock().await;
            if session.reclaim(Instant::now(), true) {
                let identifier = session.identifier.clone();
                session.close().await;
                drop(session);
                self.remove(&identifier);
            }
        }
    }

    pub(super) async fn stream(&self, text: &str, remote: SocketAddr) -> Reply {
        let request = match StreamRequest::parse(text, self.carrot) {
            Ok(request) => request,
            Err(error) => return internal(&error),
        };
        let _lock = self.stream_lock.lock().await;
        if self.shutting_down.get() {
            return reply(
                StatusCode::SERVICE_UNAVAILABLE,
                "Service shutting down".to_owned(),
                false,
                false,
            );
        }
        if self.carrot {
            self.prune().await;
        }
        let key = request.key(&remote.ip().to_string());
        let road = self.carrot && request.cameras.contains(&crate::video::ipc::Camera::Road);
        let mut old = Vec::new();
        let mut foreign = Vec::new();
        for session in self.streams.borrow().iter() {
            if road && session.road {
                if session.client_key == key {
                    old.push(Rc::clone(session));
                } else {
                    foreign.push(Rc::clone(session));
                }
            } else if !self.carrot && session.client_key == key {
                old.push(Rc::clone(session));
            }
        }
        if road && !foreign.is_empty() && !request.takeover {
            return reply(StatusCode::CONFLICT, "{\"ok\": false, \"code\": \"carrot_vision_busy\", \"error\": \"Carrot Vision is already active on another client\"}".to_owned(), true, false);
        }
        let session = match Session::new(
            &request,
            &remote.ip().to_string(),
            Profile {
                carrot: self.carrot,
                debug: self.debug,
            },
        ) {
            Ok(session) => session,
            Err(error) => return internal(&error),
        };
        if request.takeover {
            old.extend(foreign);
        }
        for session in old {
            let mut session = session.value.lock().await;
            let identifier = session.identifier.clone();
            session.close().await;
            drop(session);
            self.remove(&identifier);
        }
        let identifier = session.identifier.clone();
        let session = Rc::new(SessionHandle {
            identifier: identifier.clone(),
            client_key: session.client_key.clone(),
            road: session.road,
            value: Mutex::new(session),
        });
        self.streams.borrow_mut().push(Rc::clone(&session));
        self.sync_active();
        let answer = session.value.lock().await.answer(&self.network).await;
        match answer {
            Ok(answer) => reply(
                StatusCode::OK,
                serde_json::json!({"sdp": answer.sdp, "type": "answer"}).to_string(),
                true,
                true,
            ),
            Err(error) => {
                session.value.lock().await.close().await;
                self.remove(&identifier);
                internal(&error)
            }
        }
    }

    pub(super) async fn maintain(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut buffer = vec![0_u8; 65_536].into_boxed_slice();
        let mut prune = Instant::now();
        loop {
            interval.tick().await;
            let should_prune = prune.elapsed() >= Duration::from_secs(1);
            if should_prune {
                prune = Instant::now();
            }
            let prune_lock = if should_prune && self.carrot {
                self.stream_lock.try_lock().ok()
            } else {
                None
            };
            let should_prune = should_prune && (!self.carrot || prune_lock.is_some());
            let streams = self.streams.borrow().clone();
            for session in streams {
                let Ok(mut session) = session.value.try_lock() else {
                    continue;
                };
                if let Err(error) = session.drive(&mut self.publishers.borrow_mut(), &mut buffer) {
                    eprintln!("WebRTC session update failed: {error}");
                }
                if session.reclaim(Instant::now(), should_prune) {
                    let identifier = session.identifier.clone();
                    session.close().await;
                    drop(session);
                    self.remove(&identifier);
                }
            }
        }
    }

    pub(super) async fn shutdown(&self) {
        self.shutting_down.set(true);
        if let Err(error) = self.network.close() {
            eprintln!("WebRTC mDNS shutdown failed: {error}");
        }
        let streams = self.streams.borrow().clone();
        for session in streams {
            session.value.lock().await.close().await;
        }
        self.streams.borrow_mut().clear();
        self.set_active(false);
    }
}
