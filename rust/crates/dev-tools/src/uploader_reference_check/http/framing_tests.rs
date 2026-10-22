use super::{receive, Request, Result};
use std::{
    io::Write,
    net::{Shutdown, TcpListener, TcpStream},
    thread,
    time::Duration,
};

fn parse(parts: &[&[u8]]) -> Result<Request> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let parts = parts.iter().map(|part| part.to_vec()).collect::<Vec<_>>();
    let sender = thread::spawn(move || -> std::io::Result<()> {
        let mut stream = TcpStream::connect(address)?;
        for part in parts {
            stream.write_all(&part)?;
            stream.flush()?;
            thread::sleep(Duration::from_millis(5));
        }
        stream.shutdown(Shutdown::Write)
    });
    let (mut stream, _) = listener.accept()?;
    let result = receive(&mut stream);
    drop(stream);
    if let Err(error) = sender.join().map_err(|_| "HTTP framing sender panicked")? {
        if !matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::NotConnected
        ) {
            return Err(error.into());
        }
    }
    result
}

#[test]
fn accepts_empty_and_split_chunked_put_bodies() {
    let empty =
        parse(&[b"PUT /final HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"]).unwrap();
    assert_eq!(empty.method, "PUT");
    assert_eq!(empty.path, "/final");
    assert_eq!(empty.body_hex, "");
    let split = parse(&[
        b"PUT /final HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
        b"3\r\nab",
        b"c\r\n",
        b"2\r\nd",
        b"e\r\n0\r\n\r\n",
    ])
    .unwrap();
    assert_eq!(split.body_hex, "6162636465");
}

#[test]
fn rejects_ambiguous_unknown_oversized_and_malformed_framing() {
    for request in [
        "PUT / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n0\r\n\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: chunked\r\ntransfer-encoding: chunked\r\n\r\n0\r\n\r\n",
        "PUT / HTTP/1.1\r\nContent-Length: 0\r\ncontent-length: 0\r\n\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n400001\r\n",
        "PUT / HTTP/1.1\r\nContent-Length: 4194305\r\n\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\naXX0\r\n\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nG\r\n",
        "PUT / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n",
    ] {
        assert!(parse(&[request.as_bytes()]).is_err(), "admitted {request:?}");
    }
}
