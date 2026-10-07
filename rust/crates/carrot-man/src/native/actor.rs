use super::{
    clock,
    config::Config,
    parameters::{self, Writes},
};
use crate::{
    curve::VisionCurveSpeed,
    geos::Geos,
    owner::{packet, Owner},
    Error,
};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
#[path = "actor_tick.rs"]
mod iteration;

pub enum Action {
    PeerTcp(u64, SocketAddr),
    ClearTcp(u64),
    PeerFallback(crate::ingress::peers::Fallback, SocketAddr),
    ClearFallback(crate::ingress::peers::Fallback),
    Naver(Box<crate::sources::Snapshot>),
    Legacy(Box<packet::LegacyFrame>, String, f64),
    Udp(Box<packet::Status>, String, f64),
    TransportLost(crate::sources::Source, String, f64),
    Route(Vec<(f64, f64)>, bool),
    Health,
    Kisa(super::network::Kisa),
    Exception,
}
pub struct Request {
    pub action: Action,
    pub reply: mpsc::Sender<Result<Response, Error>>,
}
pub enum Response {
    Unit,
    Accepted(bool),
    Health(serde_json::Value),
}
#[derive(Clone)]
pub struct Handle {
    pub sender: mpsc::Sender<Request>,
    pub stop: Arc<AtomicBool>,
}
impl Handle {
    pub fn call(&self, action: Action) -> Result<Response, Error> {
        let (reply, receive) = mpsc::channel();
        self.sender
            .send(Request { action, reply })
            .map_err(|_| Error::Contract("CarrotMan owner stopped"))?;
        receive
            .recv()
            .map_err(|_| Error::Contract("CarrotMan owner stopped"))?
    }
}

pub fn run(config: Config) -> Result<(), Error> {
    if std::path::Path::new("/TICI").is_file() {
        let mut cores = rustix::thread::CpuSet::new();
        for core in 0..4 {
            cores.set(core);
        }
        if let Err(error) = rustix::thread::sched_setaffinity(None, &cores) {
            eprintln!("carrot_man failed to set core affinity: {error}");
        }
    }
    let params = parameters::open(&config)?;
    let writes = Writes::new(&config)?;
    let mut owner = Owner::new(crate::serv::settings::Settings::read(&params)?);
    let gps = if params.get_bool("UbloxAvailable")? {
        "gpsLocationExternal"
    } else {
        "gpsLocation"
    };
    let topics = [
        "deviceState",
        "carState",
        "controlsState",
        "radarState",
        "longitudinalPlan",
        "modelV2",
        "selfdriveState",
        "carControl",
        "navRouteNavd",
        gps,
        "navInstruction",
        "carrotNavi",
    ];
    let mut sub = SubMaster::for_runtime(&topics, Options::default())?;
    let mut publisher = PubMaster::for_runtime(&["carrotMan", "navRoute", "navInstructionCarrot"])?;
    let mut vision = VisionCurveSpeed::default();
    let geos = config
        .geos_library
        .as_ref()
        .map(|p| Geos::open(p))
        .transpose()?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    let (sender, receive) = mpsc::channel();
    let handle = Handle {
        sender,
        stop: Arc::clone(&stop),
    };
    super::network::start(config.clone(), handle.clone())?;
    super::http::start(config.clone(), handle.clone())?;
    let network_connected = Arc::new(AtomicBool::new(false));
    super::diagnostics::start(
        config.clone(),
        Arc::clone(&stop),
        Arc::clone(&network_connected),
    )?;
    super::upload::Upload {
        config: &config,
        params: &params,
    }
    .save_toggles();
    let mut broadcast = super::broadcast::Broadcast::new()?;
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= next {
            next += Duration::from_millis(50);
            let result = iteration::tick(iteration::Tick {
                owner: &mut owner,
                sub: &mut sub,
                publisher: &mut publisher,
                vision: &mut vision,
                geos: geos.as_ref(),
                params: &params,
                writes: &writes,
                config: &config,
                network_connected: &network_connected,
                broadcast: &mut broadcast,
                gps,
            });
            if let Err(error) = result {
                eprintln!("carrot_man broadcast loop: {error}");
                super::diagnostics::queue_exception(&params, &writes, "tmux_send");
                next = Instant::now() + Duration::from_secs(1);
            }
            continue;
        }
        match receive.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(request) => {
                let result = dispatch(&mut owner, request.action, &params, &writes, &mut publisher);
                let _ = request.reply.send(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn dispatch(
    owner: &mut Owner,
    action: Action,
    params: &openpilot_params::Params,
    writes: &Writes,
    publisher: &mut PubMaster,
) -> Result<Response, Error> {
    match action {
        Action::PeerTcp(token, peer) => {
            owner.peers.set_tcp(token, peer);
        }
        Action::ClearTcp(token) => {
            owner.peers.clear_tcp(token);
        }
        Action::PeerFallback(kind, peer) => {
            owner.peers.set_fallback(kind, peer);
        }
        Action::ClearFallback(kind) => {
            owner.peers.clear_fallback(kind);
        }
        Action::Naver(snapshot) => {
            return Ok(Response::Accepted(owner.navigation.accept(*snapshot)));
        }
        Action::Legacy(frame, session, now) => {
            super::dispatch::frame(owner, *frame, &session, now, params, writes)?;
        }
        Action::Udp(status, session, now) => {
            super::dispatch::status(owner, *status, &session, now, params)?;
        }
        Action::TransportLost(source, session, now) => {
            owner
                .navigation
                .store
                .record_transport_loss(source, &session, now);
        }
        Action::Route(points, complete) => {
            if let Some(coordinates) = super::dispatch::binary_route(owner, points, complete)? {
                publisher.send(
                    "navRoute",
                    &crate::wire::route(&coordinates, clock::timestamp()?)?,
                )?;
                super::dispatch::route_destination(owner, params)?;
            }
        }
        Action::Health => return Ok(Response::Health(owner.events.health())),
        Action::Kisa(data) => {
            if let Err(error) = super::network::apply_kisa(&mut owner.serv, data) {
                super::diagnostics::queue_exception(params, writes, "tmux_send");
                return Err(error);
            }
        }
        Action::Exception => {
            super::diagnostics::queue_exception(params, writes, "tmux_send");
        }
    }
    Ok(Response::Unit)
}
