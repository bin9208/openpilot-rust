use super::*;
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) requests: Vec<(String, String, String)>,
    pub(super) collections: HashSet<String>,
    pub(super) files: HashMap<String, Vec<u8>>,
    pub(super) fail_method: Option<(&'static str, u16)>,
    pub(super) redirect: String,
    pub(super) interrupt_put: bool,
    pub(super) puts: usize,
}

pub(super) struct Fixture {
    pub(super) client: WebDav,
    state: Arc<Mutex<State>>,
    done: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    pub(super) fn new(state: State) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let owned_done = done.clone();
        let state = Arc::new(Mutex::new(state));
        let owned_state = state.clone();
        let worker = thread::spawn(move || loop {
            match listener.accept() {
                Ok((socket, _)) => {
                    if owned_done.load(Ordering::Acquire) {
                        break;
                    }
                    serve(socket, &owned_state);
                }
                Err(error) => panic!("fixture accept: {error}"),
            }
        });
        Self {
            client: WebDav {
                base: url::Url::parse(&format!("http://{address}/openpilot/")).unwrap(),
                authorization: "Basic Zml4dHVyZS11c2VyOmZpeHR1cmUtc2VjcmV0".into(),
                agent: agent(),
            },
            state,
            done,
            worker: Some(worker),
        }
    }
    pub(super) fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        TcpStream::connect(self.client.base.socket_addrs(|| None).unwrap()[0]).unwrap();
        self.worker.take().unwrap().join().unwrap();
    }
}

fn serve(socket: TcpStream, state: &Mutex<State>) {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(socket);
    let mut first = String::new();
    if reader.read_line(&mut first).unwrap() == 0 {
        return;
    }
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap().to_owned();
    let path = parts.next().unwrap().to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        headers.insert(name.to_lowercase(), value.trim().to_owned());
    }
    let mut size = headers
        .get("content-length")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let interrupted = {
        let mut current = state.lock().unwrap();
        current.requests.push((
            method.clone(),
            path.clone(),
            headers.get("authorization").cloned().unwrap_or_default(),
        ));
        if method == "PUT" {
            current.puts += 1;
        }
        current.interrupt_put && method == "PUT" && current.puts == 1
    };
    if interrupted {
        size = size.min(1024);
    }
    let mut body = Vec::new();
    reader
        .by_ref()
        .take(u64::try_from(size).unwrap())
        .read_to_end(&mut body)
        .unwrap();
    let mut current = state.lock().unwrap();
    if method == "PUT" {
        current.files.insert(path.clone(), body);
    }
    if interrupted {
        return;
    }
    let status = if let Some((failed, status)) =
        current.fail_method.filter(|(failed, _)| *failed == method)
    {
        assert_eq!(failed, method);
        status
    } else {
        match method.as_str() {
            "OPTIONS" => 200,
            "MKCOL" => {
                if current.collections.insert(path.clone()) {
                    201
                } else {
                    405
                }
            }
            "PUT" => 201,
            "MOVE" => {
                let destination = url::Url::parse(&headers["destination"]).unwrap();
                assert_eq!(
                    destination.origin(),
                    url::Url::parse(&format!("http://{}", headers["host"]))
                        .unwrap()
                        .origin()
                );
                if let Some(body) = current.files.remove(&path) {
                    current.files.insert(destination.path().to_owned(), body);
                    201
                } else {
                    404
                }
            }
            "DELETE" => {
                if current.files.remove(&path).is_some() {
                    204
                } else {
                    404
                }
            }
            _ => 500,
        }
    };
    let location = if current.redirect.is_empty() {
        String::new()
    } else {
        format!("Location: {}\r\n", current.redirect)
    };
    drop(current);
    if let Err(error) = write!(reader.get_mut(), "HTTP/1.1 {status} Fixture\r\nContent-Length: 0\r\nConnection: close\r\nDAV: 1, 2\r\n{location}\r\n") {
        assert!(matches!(error.kind(), io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset));
    }
}

pub(super) fn file(root: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(root.join(name), bytes).unwrap();
}
