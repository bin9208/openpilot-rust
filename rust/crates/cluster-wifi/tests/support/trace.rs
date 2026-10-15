use openpilot_cluster_wifi::{Connection, Registration, Socket};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    io::{self, Read, Write},
    rc::Rc,
};

#[derive(Default)]
struct State {
    data: Vec<u8>,
    fragment: usize,
    offset: usize,
    reads: Vec<usize>,
    writes: Vec<Vec<u8>>,
    calls: Vec<Value>,
}
#[derive(Clone)]
struct Pipe(Rc<RefCell<State>>);
impl Read for Pipe {
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        state.reads.push(target.len());
        let count = target
            .len()
            .min(state.fragment)
            .min(state.data.len() - state.offset);
        target[..count].copy_from_slice(&state.data[state.offset..state.offset + count]);
        state.offset += count;
        Ok(count)
    }
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().writes.push(bytes.to_vec());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Socket for Pipe {
    fn nodelay(&mut self) -> io::Result<()> {
        self.0.borrow_mut().calls.push(json!(["option", 6, 1, 1]));
        Ok(())
    }
    fn close(&mut self) -> io::Result<()> {
        self.0
            .borrow_mut()
            .calls
            .extend([json!(["shutdown", 2]), json!(["close"])]);
        Ok(())
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn key(value: &Value) -> [u8; 32] {
    serde_json::from_value(value.clone()).unwrap()
}
pub fn run(case: &Value) -> Value {
    match case["kind"].as_str().unwrap() {
        "config_missing" => {
            let directory = tempfile::tempdir().unwrap();
            assert!(Registration::load(&directory.path().join("missing"))
                .unwrap()
                .is_none());
            json!({"config":null})
        }
        "config" => match Registration::parse(case["raw"].as_str().unwrap()) {
            Ok(config) => {
                json!({"config":config.raw(),"device":config.device,"key":config.key,"fps":config.fps,"quality":config.quality})
            }
            Err(error) => json!({"error":error.to_string()}),
        },
        "config_save" => {
            use std::os::unix::fs::PermissionsExt;
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("registration.json");
            std::fs::write(&path, "unchanged").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let mut result = match openpilot_cluster_wifi::save_config(&case["config"], &path) {
                Ok(()) => json!({"saved":true}),
                Err(error) => json!({"error":error.to_string()}),
            };
            result["file"] = json!(std::fs::read_to_string(&path).unwrap());
            result["mode"] = json!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777);
            result["temporary_files"] =
                json!(std::fs::read_dir(directory.path()).unwrap().count() - 1);
            result
        }
        "send" => {
            let pipe = Pipe(Rc::new(RefCell::new(State::default())));
            let mut connection = Connection::authenticated(
                pipe.clone(),
                key(&case["key"]),
                case["sequence"].as_u64().unwrap(),
            );
            let seed = case["seed"].as_u64().unwrap();
            let bytes: Vec<_> = (0..case["size"].as_u64().unwrap())
                .map(|i| ((i * 17 + seed) % 256) as u8)
                .collect();
            match connection.send(&bytes, case["reply"].as_bool().unwrap()) {
                Ok(sequence) => {
                    let state = pipe.0.borrow();
                    let wire: Vec<_> = state.writes.iter().flatten().copied().collect();
                    json!({"sequence":sequence,"length":wire.len(),"sha256":hex(&Sha256::digest(&wire)),"prefix":hex(&wire[..56])})
                }
                Err(error) => {
                    json!({"error":error.to_string(),"sequence":connection.sequence(),"writes":pipe.0.borrow().writes.len()})
                }
            }
        }
        "ack" | "session" => {
            let pipe = Pipe(Rc::new(RefCell::new(State {
                data: openpilot_cluster_wifi::decode_hex(case["wire"].as_str().unwrap()).unwrap(),
                fragment: case["fragment"].as_u64().unwrap() as usize,
                ..State::default()
            })));
            let mut result = if case["kind"] == "ack" {
                let mut connection = Connection::authenticated(pipe.clone(), key(&case["key"]), 0);
                // ACK fixture starts from a preauthenticated source object and
                // excludes constructor/destructor lifecycle calls.
                match connection.result() {
                    Ok((sequence, response, usb_us)) => {
                        json!({"sequence":sequence,"response":hex(&response),"usb_us":usb_us})
                    }
                    Err(error) => json!({"error":error.to_string()}),
                }
            } else {
                let calls = Cell::new(0);
                let nonce = key(&case["nonce"]);
                let config = Registration::from_value(case["config"].clone()).unwrap();
                let connection = Connection::establish(pipe.clone(), &config, || {
                    calls.set(calls.get() + 1);
                    Ok(nonce)
                });
                let mut result = match &connection {
                    Ok(connection) => {
                        json!({"session_key":hex(connection.session_key()),"sequence":connection.sequence()})
                    }
                    Err(error) => json!({"error":error.to_string()}),
                };
                let state = pipe.0.borrow();
                result["writes"] = json!(state.writes.iter().map(|v| hex(v)).collect::<Vec<_>>());
                result["calls"] = json!(state.calls);
                result["nonce_calls"] = json!(if calls.get() == 0 { vec![] } else { vec![32] });
                result
            };
            let state = pipe.0.borrow();
            result["consumed"] = json!(state.offset);
            result["reads"] = json!(state.reads);
            result
        }
        _ => panic!("unknown fixture kind"),
    }
}
