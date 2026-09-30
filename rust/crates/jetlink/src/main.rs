use openpilot_jetlink::{
    client::Client,
    ffs::{FfsTransport, Session},
    owner::{GadgetOwner, Server},
    Error,
};
use openpilot_params::Params;
use std::{
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
fn bool_param(params: &Params, key: &str) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(params.get(key)?.is_some_and(|v| v == b"1"))
}
fn setup_gadget() -> Result<(), Error> {
    let mut child = Command::new("sudo")
        .arg("-n")
        .arg(env::current_exe()?)
        .arg("--setup-gadget")
        .stdin(Stdio::null())
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(Error::Contract("gadget setup failed"))
            };
        }
        if Instant::now() >= until {
            child.kill()?;
            child.wait()?;
            return Err(Error::Deadline);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().skip(1).collect::<Vec<_>>() == ["--setup-gadget"] {
        openpilot_jetlink::gadget::provision_device()?;
        return Ok(());
    }
    let mut args = env::args().skip(1);
    let mut socket = PathBuf::from("/dev/shm/carrot-jetlink.sock");
    let mut iterations = None;
    match args.next().as_deref() {
        Some("--run") => {}
        _ => return Err("usage: jetlinkd-rs --run [--socket PATH] [--iterations N]".into()),
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => socket = args.next().ok_or("missing socket")?.into(),
            "--iterations" => {
                iterations = Some(args.next().ok_or("missing iterations")?.parse::<usize>()?)
            }
            _ => return Err("unknown option".into()),
        }
    }
    let lock_path = socket.with_extension("owner.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let params = Arc::new(Params::for_runtime()?);
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let mut owner = GadgetOwner::default();
    let mut session: Option<Arc<Session>> = None;
    let mut server: Option<Server> = None;
    let mut failed = false;
    let mut count = 0;
    while !stop.load(Ordering::Acquire) {
        let offroad = bool_param(&params, "IsOffroad")?;
        let mode = params
            .get("JetlinkMode")?
            .and_then(|v| String::from_utf8(v).ok())
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        let result = if offroad && mode == 0 {
            owner
                .disable(true, || {
                    if let Some(session) = &session {
                        session.teardown()?;
                    }
                    if let Some(server) = &mut server {
                        server.stop(Duration::from_secs(2))?;
                    }
                    server = None;
                    session = None;
                    Ok(())
                })
                .map(|()| failed = false)
        } else if offroad && matches!(mode, 1 | 2) && !owner.enabled() && !failed {
            let egpu = openpilot_jetlink::gadget::egpu_present(Path::new("/sys/bus/usb/devices"))?
                || bool_param(&params, "UsbGpuActive")?
                || bool_param(&params, "UsbGpuLoading")?;
            owner.enable(true, egpu, || {
                setup_gadget()?;
                let p = Arc::clone(&params);
                let opened = Session::open(
                    Path::new("/dev/ffs-carrot-jetlink"),
                    Path::new("/sys/kernel/config/usb_gadget/carrot_jetlink"),
                    Path::new("/sys/class/udc"),
                    Arc::new(move || {
                        p.get("IsOffroad")
                            .is_ok_and(|v| v.is_some_and(|v| v == b"1"))
                    }),
                )?;
                session = Some(Arc::clone(&opened));
                match fs::remove_file(&socket) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                server = Some(Server::spawn(
                    Client::new(FfsTransport::new(opened)),
                    &socket,
                )?);
                Ok(())
            })
        } else {
            Ok(())
        };
        if let Err(error) = result {
            failed = true;
            params.put(
                "JetlinkStatus",
                serde_json::to_vec(
                    &serde_json::json!({"phase":"ERROR","detail":error.to_string()}),
                )?
                .as_slice(),
            )?;
        }
        if !failed {
            let detail = server
                .as_ref()
                .map(|s| {
                    s.error
                        .lock()
                        .map(|v| v.clone())
                        .unwrap_or_else(|_| "owner status poisoned".into())
                })
                .unwrap_or_default();
            let ready = server
                .as_ref()
                .is_some_and(|s| s.ready.load(Ordering::Acquire));
            let phase = if !owner.enabled() {
                "OFF"
            } else if ready {
                "READY"
            } else if !detail.is_empty() {
                "LOST"
            } else {
                "PREPARING"
            };
            params.put("JetlinkStatus",&serde_json::to_vec(&serde_json::json!({"phase":phase,"detail":detail,"generation":server.as_ref().map(|s|openpilot_jetlink::rpc::encode_generation(s.generation)).unwrap_or_default()}))?)?;
        }
        count += 1;
        if iterations == Some(count) {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    if bool_param(&params, "IsOffroad")? {
        if let Some(session) = session {
            session.teardown()?;
        }
    }
    if let Some(mut server) = server {
        server.stop(Duration::from_secs(2))?;
    }
    Ok(())
}
