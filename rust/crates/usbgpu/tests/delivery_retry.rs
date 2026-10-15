use openpilot_usbgpu::model_delivery::{self as delivery, delivery_error, status};
use serde_json::{json, Value};
#[test]
fn typed_delivery_classes_match638_original_source_errors() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/delivery_error_source.json")).unwrap();
    assert_eq!(
        fixture["source_commit"],
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    assert_eq!(
        fixture["source_sha256"],
        "b49ab026a3fd960e7730292677c77465d678ef8f4bbf7bf0248bb3b7bd5e2d23"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 638);
    for case in cases {
        let actual = match case["kind"].as_str().unwrap() {
            "http" => delivery_error::classify(&delivery::Error::HttpStatus {
                code: u16::try_from(case["value"].as_u64().unwrap()).unwrap(),
                message: "fixture".into(),
            }),
            "errno" => delivery_error::io(&std::io::Error::from_raw_os_error(
                i32::try_from(case["value"].as_i64().unwrap()).unwrap(),
            )),
            "tls" => delivery_error::tls(&rustls::Error::InvalidCertificate(
                if case["value"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains("not yet valid")
                {
                    rustls::CertificateError::NotValidYet
                } else {
                    rustls::CertificateError::Expired
                },
            )),
            "dns" => delivery_error::http(&ureq::Error::HostNotFound),
            _ => unreachable!(),
        };
        assert_eq!(
            json!([actual.code, actual.retryable]),
            case["expected"],
            "{case}"
        );
    }
}
#[test]
fn persistent_delivery_failure_fields_keep_source_codes_and_retry_budget() {
    let root = tempfile::tempdir().unwrap();
    let mut reporter = status::Reporter::new(root.path(), 1.);
    reporter
        .error(
            status::Phase::WaitingForNetwork,
            status::Values {
                model: None,
                downloaded: None,
                detail: Some("certificate not yet valid"),
            },
            delivery_error::Classification {
                code: "clock",
                retryable: true,
            },
            3,
            30,
            2.,
        )
        .unwrap();
    let saved = status::read(root.path()).unwrap();
    assert_eq!(saved.state, status::Phase::WaitingForNetwork);
    assert_eq!(saved.error_code.as_deref(), Some("clock"));
    assert_eq!(saved.retry_count, Some(3));
    assert_eq!(saved.retry_in_seconds, Some(30));
    assert_eq!(saved.started_at, 2.);
}

#[test]
fn real_manifest503_retries_after30_seconds_and_preserves_cached_model() {
    use delivery::background::{self, Config};
    use openpilot_usbgpu::model::{Manifest, Paths};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let paths = Paths {
        models: root.path().join("models"),
        cache: root.path().join("cache"),
        assets: root.path().join("assets"),
    };
    fs::create_dir_all(&paths.cache).unwrap();
    let data = b"verified existing ONNX fixture";
    let model = Manifest {
        model_id: "fixture".into(),
        filename: "fixture.onnx".into(),
        size: u64::try_from(data.len()).unwrap(),
        sha256: format!("{:x}", Sha256::digest(data)),
        url: "https://example.invalid/fixture.onnx".into(),
    };
    fs::write(paths.cache.join(model.cache_filename()), data).unwrap();
    fs::write(
        paths.cache.join("state.json"),
        serde_json::to_vec(&json!({"active":model,"previous":null})).unwrap(),
    )
    .unwrap();
    let params = openpilot_params::Params::open(&root.path().join("params"), "fixture").unwrap();
    params.put_bool("UsbGpuHardwareSeen", true).unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let manifest_url = format!("http://{}/manifest.json", server.local_addr().unwrap());
    let body = serde_json::to_vec(&model).unwrap();
    let serve = thread::spawn(move || {
        let mut last = Instant::now();
        let mut delay = Duration::ZERO;
        for attempt in 0..2 {
            let (mut stream, _) = server.accept().unwrap();
            if attempt == 1 {
                delay = last.elapsed();
            }
            last = Instant::now();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            if attempt == 0 {
                stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            } else {
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        }
        delay
    });
    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancelled);
    let cache = paths.cache.clone();
    let devices = root.path().join("empty-usb");
    fs::create_dir(&devices).unwrap();
    let worker = thread::spawn(move || {
        background::run_retrying(
            &Config {
                paths: &paths,
                params: &params,
                devices: &devices,
                manifest_url: &manifest_url,
                network_wait_seconds: 0.,
            },
            true,
            &stop,
        )
        .unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(saved) =
            status::read(&cache).filter(|saved| saved.state == status::Phase::WaitingForNetwork)
        {
            assert_eq!(saved.error_code.as_deref(), Some("server"));
            assert_eq!(saved.retry_count, Some(1));
            assert_eq!(saved.retry_in_seconds, Some(30));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "transient HTTP failure was not reported"
        );
        thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
    let elapsed = serve.join().unwrap();
    assert!(elapsed >= Duration::from_secs(30));
    assert_eq!(status::read(&cache).unwrap().state, status::Phase::Ready);
    assert_eq!(fs::read(cache.join(model.cache_filename())).unwrap(), data);
    assert!(!cancelled.load(Ordering::Acquire));
}
