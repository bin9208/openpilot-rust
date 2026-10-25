//! #389: completion must describe one stable source version.
use openpilot_web_upload::{FolderUpload, Observer, Progress};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

struct Receiver {
    base: String,
    published: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Receiver {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let published = Arc::new(Mutex::new(Vec::new()));
        let (owned_stop, owned_published) = (stop.clone(), published.clone());
        let worker = thread::spawn(move || {
            while let Ok((socket, _)) = listener.accept() {
                if owned_stop.load(Ordering::Acquire) {
                    break;
                }
                // An aborted source reader intentionally closes an incomplete
                // chunked request. It must not publish that partial file.
                let _ = receive(socket, &owned_published);
            }
        });
        Self {
            base: format!("http://{address}"),
            published,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let address = self.base.strip_prefix("http://").unwrap();
        let _ = TcpStream::connect(address);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn line(reader: &mut impl BufRead) -> io::Result<String> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(line)
}
fn receive(socket: TcpStream, published: &Mutex<Vec<Vec<u8>>>) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    socket.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(socket);
    let first = line(&mut reader)?;
    assert!(first.starts_with("PUT "));
    let mut expected = None;
    let mut length = None;
    let mut chunked = false;
    loop {
        let value = line(&mut reader)?;
        if value == "\r\n" {
            break;
        }
        let (name, value) = value.split_once(':').unwrap();
        match name.to_ascii_lowercase().as_str() {
            "x-file-size" => expected = Some(value.trim().parse::<usize>().unwrap()),
            "content-length" => length = Some(value.trim().parse::<usize>().unwrap()),
            "transfer-encoding" => chunked = value.trim().eq_ignore_ascii_case("chunked"),
            _ => (),
        }
    }
    let expected = expected.unwrap();
    assert!(expected <= 4 * 1024 * 1024);
    let mut body = Vec::new();
    if chunked {
        loop {
            let size = line(&mut reader)?;
            let size = usize::from_str_radix(size.trim().split(';').next().unwrap(), 16).unwrap();
            if size == 0 {
                while line(&mut reader)? != "\r\n" {}
                break;
            }
            assert!(body.len() + size <= 4 * 1024 * 1024);
            let start = body.len();
            body.resize(start + size, 0);
            reader.read_exact(&mut body[start..])?;
            let mut separator = [0; 2];
            reader.read_exact(&mut separator)?;
            assert_eq!(&separator, b"\r\n");
        }
    } else {
        body.resize(length.unwrap(), 0);
        reader.read_exact(&mut body)?;
    }
    // Same byte-count acceptance rule as the actual route receiver. A mixed
    // same-length version is not rejected by inventing a stricter remote API.
    let (status, response) = if body.len() == expected {
        let size = body.len();
        published.lock().unwrap().push(body);
        (200, format!("{{\"ok\":true,\"size\":{size}}}"))
    } else {
        (400, "{\"ok\":false,\"error\":\"size mismatch\"}".into())
    };
    write!(
        reader.get_mut(),
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    )
}

fn mutation(mode: &str) {
    let receiver = Receiver::new();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qcamera.ts");
    let original = vec![b'A'; 2 * 1024 * 1024];
    let replacement = vec![b'B'; original.len()];
    fs::write(&path, &original).unwrap();
    let mut changed = false;
    let mut progress = |event: Progress<'_>| {
        if event.sent > 0 && !changed {
            match mode {
                "rewrite" => fs::write(&path, &replacement).unwrap(),
                "replace" => {
                    fs::rename(&path, root.path().join("old-file")).unwrap();
                    fs::write(&path, &replacement).unwrap();
                }
                "truncate" => fs::write(&path, []).unwrap(),
                "grow" => fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap()
                    .write_all(b"extra")
                    .unwrap(),
                _ => panic!("unknown mutation"),
            }
            changed = true;
        }
        Ok(())
    };
    let names = ["qcamera.ts".into()];
    let result = FolderUpload {
        local_folder: root.path(),
        directory: "private-device",
        remote_path: "route--0",
        base_url: &receiver.base,
        token: "private-token",
        filenames: Some(&names),
    }
    .run(&mut Observer {
        cancel: None,
        progress: Some(&mut progress),
    });
    assert!(changed);
    let published = receiver.published.lock().unwrap();
    if matches!(mode, "rewrite" | "replace") {
        assert!(
            published.iter().all(|bytes| bytes == &replacement),
            "changed attempt was published as complete: {mode}"
        );
        assert!(result.unwrap(), "stable existing retry must succeed");
        assert_eq!(published.len(), 1);
    } else {
        assert!(result.is_err(), "changed length was reported complete");
        assert!(published.is_empty());
    }
}

#[test]
fn same_length_rewrite_is_not_published_as_a_mixed_complete_file() {
    mutation("rewrite");
}
#[test]
fn pathname_replacement_is_not_published_as_the_current_source_file() {
    mutation("replace");
}
#[test]
fn changed_lengths_do_not_publish_partial_files() {
    mutation("truncate");
    mutation("grow");
}

#[test]
fn unchanged_file_is_complete_and_byte_exact() {
    let receiver = Receiver::new();
    let root = tempfile::tempdir().unwrap();
    let bytes = vec![13; 2 * 1024 * 1024];
    fs::write(root.path().join("qcamera.ts"), &bytes).unwrap();
    assert!(FolderUpload {
        local_folder: root.path(),
        directory: "private-device",
        remote_path: "route--0",
        base_url: &receiver.base,
        token: "private-token",
        filenames: Some(&["qcamera.ts".into()])
    }
    .run(&mut Observer::default())
    .unwrap());
    assert_eq!(*receiver.published.lock().unwrap(), vec![bytes]);
}

#[test]
fn cancellation_and_progress_errors_do_not_publish_a_complete_file() {
    use std::cell::Cell;
    for cancel_mode in [true, false] {
        let receiver = Receiver::new();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("qcamera.ts"), vec![7; 2 * 1024 * 1024]).unwrap();
        let stopped = Cell::new(false);
        let mut cancel = || stopped.get();
        let mut progress = |event: Progress<'_>| {
            if event.sent > 0 {
                if cancel_mode {
                    stopped.set(true);
                } else {
                    return Err(openpilot_web_upload::Error::Source(
                        "progress fixture".into(),
                    ));
                }
            }
            Ok(())
        };
        let error = FolderUpload {
            local_folder: root.path(),
            directory: "private-device",
            remote_path: "route--0",
            base_url: &receiver.base,
            token: "private-token",
            filenames: Some(&["qcamera.ts".into()]),
        }
        .run(&mut Observer {
            cancel: Some(&mut cancel),
            progress: Some(&mut progress),
        })
        .unwrap_err();
        if cancel_mode {
            assert!(matches!(error, openpilot_web_upload::Error::Canceled));
        } else {
            assert!(error.to_string().contains("progress fixture"));
        }
        assert!(receiver.published.lock().unwrap().is_empty());
    }
}
