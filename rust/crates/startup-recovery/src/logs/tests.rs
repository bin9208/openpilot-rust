use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn data(row: &Value) -> Vec<u8> {
    let mut data = Vec::new();
    for segment in row["segments"].as_array().unwrap() {
        let bytes: Vec<_> = segment["hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect();
        for _ in 0..segment["repeat"].as_u64().unwrap() {
            data.extend(&bytes);
        }
    }
    data
}
struct Out {
    bytes: Vec<u8>,
    flushes: usize,
}
impl Write for Out {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn original_bounded_logs_and_capture_all_bytes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/startup-recovery-logs-final.json"
    ))
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log");
    assert_eq!(read_error(None), "");
    assert_eq!(read_error(Some(&path)), "");
    for (i, row) in fixture["logs"].as_array().unwrap().iter().enumerate() {
        std::fs::write(&path, data(row)).unwrap();
        assert_eq!(
            read_error(Some(&path)),
            row["expected"].as_str().unwrap(),
            "log {i}"
        );
    }
    for (i, row) in fixture["captures"].as_array().unwrap().iter().enumerate() {
        let input = data(row);
        let mut reader = io::Cursor::new(input);
        let mut output = Out {
            bytes: Vec::new(),
            flushes: 0,
        };
        let mut hashes = Vec::new();
        let mut ticks = row["ticks"].as_array().unwrap().iter();
        capture(
            &mut reader,
            &mut output,
            |bytes| {
                assert!(bytes.len() <= LIMIT);
                hashes.push(digest(bytes));
                Ok(())
            },
            || ticks.next().unwrap().as_f64().unwrap(),
        )
        .unwrap();
        assert!(ticks.next().is_none());
        assert_eq!(serde_json::json!(hashes), row["writes"], "writes {i}");
        assert_eq!(
            digest(&output.bytes),
            row["forward_sha256"].as_str().unwrap(),
            "forward {i}"
        );
        assert_eq!(
            output.flushes as u64,
            row["flushes"].as_u64().unwrap(),
            "flush {i}"
        );
    }
}
