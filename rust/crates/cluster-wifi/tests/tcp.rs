//! Loopback only: authenticating fragmented real sockets, never an S3/device.
use openpilot_cluster_wifi::{connect, mac, Registration, ACK, HEADER_BYTES, HELLO, MAGIC};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn config() -> Registration {
    Registration::from_value(serde_json::json!({"version":1,"device":"001122334455","key":"abababababababababababababababababababababababababababababababab"})).unwrap()
}
fn wire(values: [u32; 6], key: &[u8; 32], payload: &[u8]) -> Vec<u8> {
    let mut header = Vec::new();
    for value in values {
        header.extend(value.to_be_bytes());
    }
    let mut signed = header.clone();
    signed.extend(payload);
    header.extend(mac(key, &signed));
    header.extend(payload);
    header
}
#[test]
fn real_loopback_fragments_authentication_packets_and_acknowledgements() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let registration = config();
    let server_registration = registration.clone();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut hello = HELLO.to_vec();
        hello.extend(server_registration.device);
        hello.extend(32u8..64);
        for fragment in hello.chunks(5) {
            stream.write_all(fragment).unwrap();
        }
        let mut request = [0; 64];
        stream.read_exact(&mut request).unwrap();
        let nonce = &request[..32];
        let mut client = b"client".to_vec();
        client.extend(&hello);
        client.extend(nonce);
        assert_eq!(&request[32..], &mac(&server_registration.key, &client));
        let mut response = b"server".to_vec();
        response.extend(&hello);
        response.extend(nonce);
        for fragment in mac(&server_registration.key, &response).chunks(5) {
            stream.write_all(fragment).unwrap();
        }
        let mut session = b"session".to_vec();
        session.extend(hello);
        session.extend(nonce);
        let session = mac(&server_registration.key, &session);
        for sequence in [1, 2] {
            let mut header = [0; HEADER_BYTES];
            stream.read_exact(&mut header).unwrap();
            let fields: Vec<_> = header
                .chunks_exact(4)
                .map(|v| u32::from_be_bytes(v.try_into().unwrap()))
                .collect();
            assert_eq!(
                fields,
                [MAGIC, sequence, 512, u32::from(sequence == 1), 0, 0]
            );
            let mut signature = [0; 32];
            stream.read_exact(&mut signature).unwrap();
            let mut payload = [0; 512];
            stream.read_exact(&mut payload).unwrap();
            assert_eq!(payload, [37; 512]);
            let mut signed = header.to_vec();
            signed.extend(payload);
            assert_eq!(signature, mac(&session, &signed));
            let data = if sequence == 1 {
                b"sourceOK".as_slice()
            } else {
                b"".as_slice()
            };
            for fragment in wire(
                [ACK, sequence, data.len() as u32, 0, 12345, 0],
                &session,
                data,
            )
            .chunks(3)
            {
                stream.write_all(fragment).unwrap();
            }
        }
        let mut last = [0];
        assert_eq!(stream.read(&mut last).unwrap(), 0);
    });
    let mut connection = connect(&registration, "127.0.0.1", address.port()).unwrap();
    assert_eq!(connection.send(&[37; 512], true).unwrap(), 1);
    assert_eq!(
        connection.result().unwrap(),
        (1, b"sourceOK".to_vec(), 12345)
    );
    assert_eq!(connection.send(&[37; 512], false).unwrap(), 2);
    assert_eq!(connection.result().unwrap(), (2, vec![], 12345));
    connection.close().unwrap();
    worker.join().unwrap();
}
#[test]
fn bad_identity_and_server_authentication_close_the_real_socket() {
    for bad_identity in [true, false] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let registration = config();
        let server_registration = registration.clone();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut hello = HELLO.to_vec();
            hello.extend(server_registration.device);
            hello.extend([0; 32]);
            if bad_identity {
                hello[4] ^= 1;
            }
            stream.write_all(&hello).unwrap();
            if !bad_identity {
                let mut request = [0; 64];
                stream.read_exact(&mut request).unwrap();
                stream.write_all(&[0; 32]).unwrap();
            }
            let mut last = [0];
            assert_eq!(stream.read(&mut last).unwrap(), 0);
        });
        let error = match connect(&registration, "127.0.0.1", address.port()) {
            Ok(_) => panic!("unauthenticated connection accepted"),
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            if bad_identity {
                "S3 device identity mismatch"
            } else {
                "S3 authentication failed"
            }
        );
        worker.join().unwrap();
    }
}
