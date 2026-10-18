//! #388: completion must not mark a pathname replacement as uploaded.
use openpilot_uploader::{
    http::{HttpTransfer, SigningKey},
    Attributes, Candidate, Outcome, Uploader, XattrCache,
};
use p256::pkcs8::{EncodePrivateKey, LineEnding};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn unmarked(path: &Path) {
    assert_eq!(
        rustix::fs::getxattr(path, "user.upload", &mut [0; 8]),
        Err(rustix::io::Errno::NODATA),
        "never-sent replacement was marked uploaded"
    );
}

struct CompletionEvents {
    path: std::path::PathBuf,
    rewrite: bool,
}
impl openpilot_uploader::EventSink for CompletionEvents {
    fn emit(&mut self, event: openpilot_uploader::Event) -> Result<(), openpilot_logging::Error> {
        if self.rewrite && event.name() == Some("upload_success") {
            fs::write(&self.path, b"rewritten content").unwrap();
            self.rewrite = false;
        }
        Ok(())
    }
}

struct CompletionAttributes {
    cache: XattrCache,
    replacement: Option<std::path::PathBuf>,
}
impl Attributes for CompletionAttributes {
    fn get(&mut self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
        self.cache.get(path)
    }
    fn mark_uploaded(&mut self, path: &Path) -> std::io::Result<()> {
        self.cache.mark_uploaded(path)
    }
    fn mark_uploaded_source(&mut self, path: &Path, source: &fs::File) -> std::io::Result<()> {
        if let Some(old) = self.replacement.take() {
            fs::rename(path, old)?;
            fs::write(path, b"new unsent body!!")?;
        }
        self.cache.mark_uploaded_source(path, source)
    }
}

fn http_completion(status: u16, mutation: &str, compressed: bool, write_only: bool) {
    let root = tempfile::tempdir().unwrap();
    let name = if compressed { "qlog" } else { "qcamera.ts" };
    let source = root.path().join(name);
    let archived = root.path().join("old-file");
    let original = b"old uploaded body";
    let replacement = b"new unsent body!!";
    let mutation = mutation.to_owned();
    let worker_mutation = mutation.clone();
    fs::write(&source, original).unwrap();
    let version = root.path().join("version.h");
    fs::write(&version, b"#define COMMA_VERSION \"fixture\"\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    if write_only {
        fs::set_permissions(&source, fs::Permissions::from_mode(0o200)).unwrap();
    }
    let path = source.clone();
    let old = archived.clone();
    let worker = thread::spawn(move || {
        let mut sent = Vec::new();
        for index in 0..if status == 412 { 1 } else { 2 } {
            let end = Instant::now() + Duration::from_secs(5);
            let socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < end, "owned HTTP request absent");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(socket);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            assert!(first.starts_with(if index == 0 { "GET " } else { "PUT " }));
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                let (name, value) = line.split_once(':').unwrap();
                if name.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            if index == 1 {
                let bytes = if compressed {
                    zstd::stream::decode_all(std::io::Cursor::new(body)).unwrap()
                } else {
                    body
                };
                assert_eq!(
                    bytes.as_slice(),
                    if worker_mutation == "aba" {
                        replacement.as_slice()
                    } else {
                        original.as_slice()
                    }
                );
                sent = bytes;
            }
            match worker_mutation.as_str() {
                "replace" if index == 1 || status == 412 => {
                    fs::rename(&path, &old).unwrap();
                    fs::write(&path, replacement).unwrap();
                }
                "rewrite" if index == 1 => {
                    fs::write(&path, original.map(|_| b'B')).unwrap();
                }
                "aba" if index == 0 => {
                    fs::rename(&path, &old).unwrap();
                    fs::write(&path, replacement).unwrap();
                }
                "aba" if index == 1 => {
                    fs::rename(&path, path.with_file_name("never-sent-replacement")).unwrap();
                    fs::rename(&old, &path).unwrap();
                }
                _ => (),
            }
            let code = if index == 0 && status != 412 {
                200
            } else {
                status
            };
            let body = if index == 0 && status != 412 {
                format!("{{\"url\":\"http://{address}/upload\",\"headers\":{{}}}}")
            } else {
                String::new()
            };
            write!(
                reader.get_mut(),
                "HTTP/1.1 {code} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        sent
    });
    let pem = p256::SecretKey::from_slice(&[7; 32])
        .unwrap()
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap();
    let transfer = HttpTransfer {
        api_host: format!("http://{address}"),
        dongle_id: "private-device".into(),
        key: Some(SigningKey::from_pem(
            jsonwebtoken::Algorithm::ES256,
            pem.as_bytes().to_vec(),
        )),
        version_header: version,
        fake_upload: false,
        socket_timeout: Duration::from_secs(3),
    };
    let mut uploader = Uploader::new(
        root.path().into(),
        transfer,
        CompletionAttributes {
            cache: XattrCache::default(),
            replacement: (mutation == "mark-race").then(|| archived.clone()),
        },
        CompletionEvents {
            path: source.clone(),
            rewrite: mutation == "log-rewrite",
        },
    );
    let result = uploader.upload(
        &Candidate {
            name: name.into(),
            key: format!("route--0/{name}{}", if compressed { ".zst" } else { "" }).into(),
            path: source.clone(),
        },
        1,
        false,
    );
    let sent = worker.join().unwrap();
    let result = result.unwrap();
    if write_only {
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    }
    if status != 412 {
        assert_eq!(
            sent.as_slice(),
            if mutation == "aba" {
                replacement.as_slice()
            } else {
                original.as_slice()
            }
        );
    }
    if mutation == "mark-race" {
        assert_eq!(result, Outcome::Success);
        unmarked(&source);
        assert_eq!(
            uploader.attributes.get(&archived).unwrap(),
            Some(b"1".to_vec())
        );
    } else if mutation != "none" {
        if mutation == "replace" {
            assert_eq!(fs::read(&source).unwrap(), replacement);
        }
        unmarked(&source);
        assert_eq!(
            result,
            Outcome::Failure,
            "changed pathname reported complete"
        );
        assert!(uploader.last_filename.as_os_str().is_empty());
    } else {
        assert_eq!(result, Outcome::Success);
        assert_eq!(
            uploader.attributes.get(&source).unwrap(),
            Some(b"1".to_vec())
        );
    }
}

#[test]
fn real_http_acknowledgement_cannot_mark_a_replacement_file() {
    for compressed in [false, true] {
        http_completion(200, "replace", compressed, false);
    }
}

#[test]
fn unchanged_files_keep_all_original_terminal_http_statuses() {
    for status in [200, 201, 401, 403, 412] {
        for compressed in [false, true] {
            http_completion(status, "none", compressed, false);
        }
    }
}

#[test]
fn ignored_upload_response_cannot_mark_a_replacement_file() {
    http_completion(412, "replace", false, false);
}

#[test]
fn same_inode_rewrite_and_aba_cannot_mark_the_wrong_source_complete() {
    for mutation in ["rewrite", "aba"] {
        for compressed in [false, true] {
            http_completion(200, mutation, compressed, false);
        }
    }
}
#[test]
fn ignored_status_does_not_require_read_permission() {
    http_completion(412, "none", false, true);
}

struct NoTransfer;
impl openpilot_uploader::Transfer for NoTransfer {
    fn upload(
        &mut self,
        _: &Path,
        _: &Path,
        _: &mut dyn openpilot_uploader::EventSink,
    ) -> Result<openpilot_uploader::UploadResponse, openpilot_uploader::TransferError> {
        panic!("empty/oversized source must not transfer")
    }
}
#[test]
fn empty_and_oversized_policy_keeps_write_only_sources_and_previous_filename() {
    for (name, size) in [("qlog", 0), ("qlog", 25_000_001), ("qcam", 5_000_001)] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join(name);
        fs::File::create(&source).unwrap().set_len(size).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o200)).unwrap();
        let mut uploader = Uploader::new(
            root.path().into(),
            NoTransfer,
            XattrCache::default(),
            Vec::new(),
        );
        uploader.last_filename = "previous-success".into();
        assert_eq!(
            uploader
                .upload(
                    &Candidate {
                        name: name.into(),
                        key: name.into(),
                        path: source.clone()
                    },
                    1,
                    false
                )
                .unwrap(),
            Outcome::Success
        );
        assert_eq!(uploader.last_filename, Path::new("previous-success"));
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            uploader.attributes.get(&source).unwrap(),
            Some(b"1".to_vec())
        );
    }
}

#[test]
fn descriptor_mark_cannot_follow_a_replacement_after_validation() {
    struct ReplaceAtMark {
        cache: XattrCache,
        old: std::path::PathBuf,
    }
    impl Attributes for ReplaceAtMark {
        fn get(&mut self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
            self.cache.get(path)
        }
        fn mark_uploaded(&mut self, _: &Path) -> std::io::Result<()> {
            panic!("completion must use the retained descriptor")
        }
        fn mark_uploaded_source(&mut self, path: &Path, source: &fs::File) -> std::io::Result<()> {
            fs::rename(path, &self.old)?;
            fs::write(path, b"never sent replacement")?;
            self.cache.mark_uploaded_source(path, source)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qlog");
    let old = root.path().join("processed-empty-file");
    fs::write(&path, []).unwrap();
    let mut uploader = Uploader::new(
        root.path().into(),
        NoTransfer,
        ReplaceAtMark {
            cache: XattrCache::default(),
            old: old.clone(),
        },
        Vec::new(),
    );
    assert_eq!(
        uploader
            .upload(
                &Candidate {
                    name: "qlog".into(),
                    key: "qlog".into(),
                    path: path.clone()
                },
                1,
                false
            )
            .unwrap(),
        Outcome::Success
    );
    unmarked(&path);
    assert_eq!(uploader.attributes.get(&old).unwrap(), Some(b"1".to_vec()));
}

#[test]
fn real_http_descriptor_mark_targets_only_the_transmitted_object() {
    for compressed in [false, true] {
        http_completion(200, "mark-race", compressed, false);
    }
}

#[test]
fn real_http_success_log_rewrite_cannot_mark_changed_source() {
    for compressed in [false, true] {
        http_completion(200, "log-rewrite", compressed, false);
    }
}
