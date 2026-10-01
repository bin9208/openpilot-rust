use crate::{
    forwarding, logging, methods, net, policy, rpc,
    state::{self, Shared, Stop},
    uploads,
    websocket::Connection,
    Error,
};
use openpilot_uploader::http::SigningKey;
use rand::Rng;
use serde_json::json;
use std::{
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tungstenite::Message;

struct Session {
    stop: Stop,
    workers: Vec<JoinHandle<()>>,
    shared: Arc<Shared>,
}
impl Session {
    fn new(shared: Arc<Shared>) -> Result<Self, Error> {
        let mut session = Self {
            stop: Stop::default(),
            workers: Vec::new(),
            shared,
        };
        for worker in 0..4 {
            session.spawn(&format!("upload_handler{worker}"), move |shared, stop| {
                uploads::run(shared, stop, worker)
            })?;
        }
        session.spawn("log_handler", |shared, stop| {
            forwarding::log_worker(shared, stop);
            Ok(())
        })?;
        session.spawn("stat_handler", |shared, stop| {
            forwarding::stats(shared, stop);
            Ok(())
        })?;
        for worker in 0..session.shared.config.handlers {
            session.spawn(&format!("worker_{worker}"), jsonrpc_worker)?;
        }
        Ok(session)
    }
    fn spawn(
        &mut self,
        name: &str,
        body: impl FnOnce(Arc<Shared>, Stop) -> Result<(), Error> + Send + 'static,
    ) -> Result<(), Error> {
        let shared = Arc::clone(&self.shared);
        let stop = self.stop.clone();
        let factory = shared.factory.clone();
        self.workers.push(
            std::thread::Builder::new()
                .name(name.into())
                .spawn(move || {
                    if let Err(error) = body(shared, stop) {
                        logging::failure(&mut factory.logger(), "athena.worker.exception", &error);
                    }
                })?,
        );
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.request();
        self.shared.available.notify_all();
        for handle in self.workers.drain(..) {
            if handle.join().is_err() {
                eprintln!("athenad: worker panicked");
            }
        }
        match self.shared.proxies.lock() {
            Ok(mut proxies) => {
                for handle in proxies.drain(..) {
                    if handle.join().is_err() {
                        eprintln!("athenad: proxy worker panicked");
                    }
                }
            }
            Err(error) => eprintln!("athenad: proxy cleanup: {error}"),
        }
    }
}
fn jsonrpc_worker(shared: Arc<Shared>, stop: Stop) -> Result<(), Error> {
    let mut logger = shared.factory.logger();
    while !stop.requested() {
        if let Some((text, binary)) = shared.requests.get(Duration::from_secs(1))? {
            if text.contains("method") {
                logging::event(
                    &mut logger,
                    "athena.jsonrpc_handler.call_method",
                    json!({"data":text}),
                )?;
            }
            let result = rpc::route(&text, binary, |method, args| {
                methods::call(&shared, &stop, &mut logger, method, args)
            });
            match result {
                rpc::Route::Reply(text) => shared.replies.put(text)?,
                rpc::Route::LogResponse(text) => shared.log_responses.put(text)?,
            }
        }
    }
    Ok(())
}
pub fn connected(shared: Arc<Shared>, global: &Stop, mut ws: Connection) -> Result<(), Error> {
    let session = Session::new(Arc::clone(&shared))?;
    let mut logger = shared.factory.logger();
    let mut ping = Instant::now();
    let mut managed = None::<Instant>;
    let mut onroad = None;
    while !global.requested() && !session.stop.requested() {
        if managed.is_none_or(|time| time.elapsed() >= Duration::from_secs(5)) {
            let current = state::boolean(&shared.params, "IsOnroad")?;
            if onroad != Some(current) {
                if let Err(error) = net::keepalive(&ws.tcp, current) {
                    logging::failure(&mut logger, "athena.ws_manage.exception", &error);
                }
                onroad = Some(current);
            }
            managed = Some(Instant::now());
        }
        match ws.read() {
            Ok(Some(Message::Text(text))) => shared.requests.put((text.to_string(), false))?,
            Ok(Some(Message::Binary(_))) => shared.requests.put((String::new(), true))?,
            Ok(Some(Message::Ping(_))) => {
                ping = Instant::now();
                shared.params.put(
                    "LastAthenaPingTime",
                    state::mono_ns()?.to_string().as_bytes(),
                )?;
            }
            Ok(Some(Message::Close(_))) => break,
            Ok(Some(Message::Pong(_) | Message::Frame(_))) => {}
            Ok(None) => {
                if ws.read_timeout()? && ping.elapsed() > Duration::from_secs(70) {
                    logging::failure(
                        &mut logger,
                        "athenad.ws_recv.timeout",
                        &"last ping exceeded 70 seconds",
                    );
                    break;
                }
            }
            Err(error) => {
                logging::failure(&mut logger, "athenad.ws_recv.exception", &error);
                break;
            }
        }
        let outgoing = match shared.replies.get(Duration::ZERO)? {
            Some(reply) => Some(reply),
            None => shared.low_priority.get(Duration::ZERO)?,
        };
        if let Some(text) = outgoing {
            if let Err(error) = ws.text(&text) {
                logging::failure(&mut logger, "athenad.ws_send.exception", &error);
                break;
            }
        }
        if let Err(error) = ws.flush() {
            logging::failure(&mut logger, "athenad.ws_send.exception", &error);
            break;
        }
        global.wait(Duration::from_millis(5));
    }
    drop(session);
    Ok(())
}
pub fn run(shared: Arc<Shared>, stop: &Stop) -> Result<(), Error> {
    let mut logger = shared.factory.logger();
    if !shared.config.pc {
        let mut cores = rustix::thread::CpuSet::new();
        for core in 0..4 {
            cores.set(core);
        }
        if let Err(error) = rustix::thread::sched_setaffinity(None, &cores) {
            logging::failure(&mut logger, "failed to set core affinity", &error);
        }
    }
    let id = shared
        .text("DongleId", &mut logger)?
        .ok_or(Error::Contract("DongleId missing"))?;
    if let Some(cache) = state::read(&shared.params, "AthenadUploadQueue")? {
        if let Err(error) = shared.uploads()?.initialize(&cache) {
            logging::failure(
                &mut logger,
                "athena.UploadQueueCache.initialize.exception",
                &error,
            );
        }
    }
    let key = SigningKey::load(&shared.config.persist_root)?;
    let uri = format!("{}/ws/v2/{id}", shared.config.host);
    let mut retries = 0_u32;
    let mut started = None::<Instant>;
    while !stop.requested() {
        let time = *started.get_or_insert_with(Instant::now);
        logging::event(
            &mut logger,
            "athenad.main.connecting_ws",
            json!({"ws_uri":uri,"retries":retries}),
        )?;
        let connection = (|| {
            let token = key
                .as_ref()
                .ok_or(Error::Contract("signing key unavailable"))?
                .token(
                    &id,
                    u64::try_from(state::now_ms()? / 1000)
                        .map_err(|_| Error::Contract("negative wall clock"))?,
                )?;
            Connection::connect(&uri, &token)
        })();
        match connection {
            Ok(ws) => {
                logging::event(
                    &mut logger,
                    "athenad.main.connected_ws",
                    json!({"ws_uri":uri,"retries":retries,"duration":time.elapsed().as_secs_f64()}),
                )?;
                started = None;
                retries = 0;
                shared.uploads()?.current.clear();
                if let Err(error) = connected(Arc::clone(&shared), stop, ws) {
                    logging::failure(&mut logger, "athenad.main.exception", &error);
                    retries = retries.saturating_add(1);
                    state::remove(&shared.params, "LastAthenaPingTime")?;
                }
            }
            Err(error) => {
                logging::failure(&mut logger, "athenad.main.exception", &error);
                retries = retries.saturating_add(1);
                state::remove(&shared.params, "LastAthenaPingTime")?;
            }
        }
        stop.wait(Duration::from_secs(u64::from(
            rand::rng().random_range(0..policy::backoff_limit(retries)),
        )));
    }
    Ok(())
}
