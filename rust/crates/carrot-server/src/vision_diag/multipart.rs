use super::format::Files;

pub(super) struct Body {
    pub content_type: String,
    pub bytes: Vec<u8>,
}
fn filename(name: &str) -> String {
    let mut output = String::new();
    for byte in name.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(byte) {
            output.push(char::from(*byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    output
}
pub(super) fn encode(payload: &str, files: &Files) -> Body {
    let boundary = uuid::Uuid::new_v4().simple().to_string();
    let mut bytes = format!("--{boundary}\r\nContent-Type: application/json\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\r\n{payload}\r\n").into_bytes();
    for (field, name, data) in [
        ("files[0]", &files.name, &files.bytes),
        ("files[1]", &files.console_name, &files.console_bytes),
    ] {
        if data.is_empty() {
            continue;
        }
        bytes.extend_from_slice(format!("--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{}\"\r\n\r\n", filename(name)).as_bytes());
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Body {
        content_type: format!("multipart/form-data; boundary={boundary}"),
        bytes,
    }
}
