use crate::{vision_socket, vision_wire, Error, VisionClient, VisionLayout, VisionStream};
use std::{
    env,
    fs::File,
    os::fd::{AsFd, OwnedFd},
    sync::{atomic::AtomicBool, Arc},
    thread::JoinHandle,
};

fn isolated(name: &str) -> bool {
    if env::var("IPC194_VISION_BOUNDARY").as_deref() == Ok(name) {
        return true;
    }
    let namespace = tempfile::Builder::new()
        .prefix("msgq_rust-v194-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = namespace
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let full_name = format!("vision_boundary_tests::{name}");
    let status = crate::test_process::command(env::current_exe().unwrap())
        .args(["--exact", &full_name, "--nocapture"])
        .env("IPC194_VISION_BOUNDARY", name)
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status()
        .unwrap();
    assert!(status.success());
    false
}

fn buffer(index: usize) -> vision_wire::Buffer {
    vision_wire::Buffer {
        layout: VisionLayout {
            width: 8,
            height: 4,
            stride: 16,
            uv_offset: 64,
            len: 96,
        },
        mapped_length: 104,
        server_id: 1234,
        index,
        stream: VisionStream::Road,
    }
}

fn serve(payload: Vec<u8>, descriptors: Vec<OwnedFd>) -> JoinHandle<()> {
    let path = vision_socket::path("boundary").unwrap();
    let socket = vision_socket::bind(&path).unwrap();
    std::thread::spawn(move || {
        let client = vision_socket::accept(socket.as_fd()).unwrap();
        let (request, rights) = vision_socket::receive(client.as_fd(), 4).unwrap();
        assert_eq!(request.len(), 4);
        assert!(rights.is_empty());
        vision_socket::send(client.as_fd(), &payload, &descriptors).unwrap();
        drop(client);
        drop(socket);
        std::fs::remove_file(path).unwrap();
    })
}

fn file(length: u64) -> File {
    let file = tempfile::tempfile().unwrap();
    file.set_len(length).unwrap();
    file
}

#[test]
fn malformed_handshake_rejects_foreign_layouts_and_closes_received_rights() {
    if !isolated("malformed_handshake_rejects_foreign_layouts_and_closes_received_rights") {
        return;
    }
    let original = file(104);
    for case in 0..7 {
        let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
        let mut payload = vision_wire::encode_buffer(buffer(0), 0xffff_ffff_ffff_0000, 99)
            .unwrap()
            .to_vec();
        let mut descriptors = vec![original.as_fd().try_clone_to_owned().unwrap()];
        match case {
            0 => {
                payload.pop();
            }
            1 => {
                descriptors.push(original.as_fd().try_clone_to_owned().unwrap());
            }
            2 => payload[40..48].copy_from_slice(&7_u64.to_le_bytes()),
            3 => payload[8..16].copy_from_slice(&103_u64.to_le_bytes()),
            4 => payload[96..104].copy_from_slice(&1_u64.to_le_bytes()),
            5 => payload[104..108].copy_from_slice(&4_i32.to_le_bytes()),
            6 => descriptors.clear(),
            _ => unreachable!(),
        }
        let server = serve(payload, descriptors);
        let mut client = VisionClient::new("boundary", VisionStream::Road, false).unwrap();

        assert!(client.connect().is_err(), "case {case}");

        assert!(!client.is_connected());
        assert!(client.layout().is_none());
        assert!(client.receive_retained(std::time::Duration::ZERO).is_err());
        server.join().unwrap();
        drop(client);
        assert_eq!(
            std::fs::read_dir("/proc/self/fd").unwrap().count(),
            before,
            "case {case}"
        );
    }
}

#[test]
fn partial_import_failure_releases_mappings_and_all_transferred_descriptors() {
    if !isolated("partial_import_failure_releases_mappings_and_all_transferred_descriptors") {
        return;
    }
    for short in [true, false] {
        let first = file(104);
        let second = if short {
            file(8)
        } else {
            File::open("/dev/null").unwrap()
        };
        let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
        let mut payload = vision_wire::encode_buffer(buffer(0), 0x1000, 1)
            .unwrap()
            .to_vec();
        payload.extend_from_slice(&vision_wire::encode_buffer(buffer(1), 0x2000, 2).unwrap());
        let descriptors = vec![
            first.as_fd().try_clone_to_owned().unwrap(),
            second.as_fd().try_clone_to_owned().unwrap(),
        ];
        let server = serve(payload, descriptors);
        let mut client = VisionClient::new("boundary", VisionStream::Road, false).unwrap();

        assert!(matches!(client.connect(), Err(Error::Corrupt(_))));

        server.join().unwrap();
        assert!(client.layout().is_none());
        drop(client);
        assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
    }
}

#[test]
fn discovery_rejects_unexpected_descriptors_without_leaking_them() {
    if !isolated("discovery_rejects_unexpected_descriptors_without_leaking_them") {
        return;
    }
    let original = file(104);
    let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
    let server = serve(
        0_i32.to_le_bytes().to_vec(),
        vec![original.as_fd().try_clone_to_owned().unwrap()],
    );

    assert!(matches!(
        VisionClient::available_streams("boundary"),
        Err(Error::Corrupt(_))
    ));

    server.join().unwrap();
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
}

#[test]
fn listener_shutdown_releases_an_accepted_client_with_no_request() {
    if !isolated("listener_shutdown_releases_an_accepted_client_with_no_request") {
        return;
    }
    let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
    let path = vision_socket::path("boundary").unwrap();
    let socket = vision_socket::bind(&path).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let (accepted, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let client = vision_socket::accept(socket.as_fd()).unwrap();
        accepted.send(()).unwrap();
        assert!(!vision_socket::wait(client.as_fd(), &worker_stop).unwrap());
    });
    let client = vision_socket::connect(&path).unwrap().unwrap();
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();

    stop.store(true, std::sync::atomic::Ordering::Release);
    worker.join().unwrap();

    drop(client);
    std::fs::remove_file(path).unwrap();
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
}
