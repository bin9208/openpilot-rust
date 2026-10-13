use openpilot_cluster::autorun::{
    controller::{self, Platform},
    entry::{Entrypoint, Failure},
    events::Events,
    params::Reader,
    RunRequest,
};
use openpilot_params::Params;
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, Read},
    os::unix::net::UnixDatagram,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(serde::Deserialize)]
struct Case {
    root: PathBuf,
    prefix: String,
    board: bool,
    found: Vec<bool>,
    entries: Vec<Entry>,
    packets: Vec<Vec<u8>>,
    update: Option<Update>,
}

#[derive(serde::Deserialize)]
struct Update {
    delay_ms: u64,
    values: Vec<(String, String)>,
}

#[derive(serde::Deserialize)]
struct Entry {
    result: Outcome,
    values: Vec<(String, String)>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Success,
    H264,
    Runtime,
    Disconnected,
}

type Trace = Rc<RefCell<Vec<serde_json::Value>>>;

struct Recipient {
    trace: Trace,
    params: Params,
    entries: VecDeque<Entry>,
}

impl Entrypoint for Recipient {
    fn run(&mut self, request: RunRequest) -> Result<(), Failure> {
        self.trace
            .borrow_mut()
            .push(serde_json::json!({"entry":request.args()}));
        let action = self
            .entries
            .pop_front()
            .ok_or(openpilot_cluster::Error::Contract(
                "entry recipient exhausted",
            ))?;
        for (key, value) in action.values {
            self.params
                .put(&key, value.as_bytes())
                .map_err(openpilot_cluster::Error::from)?;
        }
        match action.result {
            Outcome::Success => Ok(()),
            Outcome::H264 => Err(Failure::H264Initialization(
                openpilot_cluster::Error::Contract("owned H264 initialization failure"),
            )),
            Outcome::Runtime => {
                Err(openpilot_cluster::Error::Contract("owned runtime failure").into())
            }
            Outcome::Disconnected => Err(openpilot_cluster::Error::Disconnected(Box::new(
                openpilot_cluster::Error::Contract("owned unplug"),
            ))
            .into()),
        }
    }
}

struct OwnedPlatform {
    trace: Trace,
    board: bool,
    found: VecDeque<bool>,
    packets: Vec<Vec<u8>>,
}

impl Platform for OwnedPlatform {
    fn board(&self) -> bool {
        self.board
    }
    fn offroad(&mut self) -> Result<(), controller::Error> {
        self.trace.borrow_mut().push(serde_json::json!("offroad"));
        Ok(())
    }
    fn startup(&mut self, _: &Reader) -> Result<(), controller::Error> {
        self.trace.borrow_mut().push(serde_json::json!("startup"));
        Ok(())
    }
    fn find(&mut self, product: u16) -> Option<u16> {
        self.trace
            .borrow_mut()
            .push(serde_json::json!({"find":product}));
        self.found.pop_front().unwrap_or(false).then_some(product)
    }
    fn dim(&mut self, product: u16, reason: &str) -> bool {
        self.trace
            .borrow_mut()
            .push(serde_json::json!({"dim":product,"reason":reason}));
        true
    }
    fn events(&mut self) -> Events {
        self.trace.borrow_mut().push(serde_json::json!("events"));
        let (reader, writer) = UnixDatagram::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        for packet in &self.packets {
            writer.send(packet).unwrap();
        }
        Events::from_socket(reader.into())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let params = Params::open(&case.root, &case.prefix)?;
    let updater = case.update.map(|update| {
        let params = params.clone();
        std::thread::spawn(move || -> Result<(), openpilot_params::Error> {
            std::thread::sleep(Duration::from_millis(update.delay_ms));
            for (key, value) in update.values {
                params.put(&key, value.as_bytes())?;
            }
            Ok(())
        })
    });
    let trace = Rc::new(RefCell::new(vec![serde_json::json!("offroad")]));
    let mut platform = OwnedPlatform {
        trace: trace.clone(),
        board: case.board,
        found: case.found.into(),
        packets: case.packets,
    };
    let mut entry = Recipient {
        trace: trace.clone(),
        params: params.clone(),
        entries: case.entries.into(),
    };
    let reader = Reader::new(params.clone());
    let started = Instant::now();
    controller::run(&reader, &mut platform, &mut entry)?;
    let elapsed = started.elapsed();
    drop(reader);
    if let Some(updater) = updater {
        updater
            .join()
            .map_err(|_| io::Error::other("Params updater panicked"))??;
    }
    println!(
        "{}",
        serde_json::json!({"trace":*trace.borrow(),"connected":params.get_bool("ClusterHudConnected")?,"elapsed_ms":elapsed.as_secs_f64()*1000.0})
    );
    Ok(())
}
