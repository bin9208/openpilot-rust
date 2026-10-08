use qrcodegen::QrSegment;
fn alpha(byte: u8) -> bool {
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:".contains(&byte)
}
fn split(data: &[u8], predicate: impl Fn(u8) -> bool) -> Vec<(bool, &[u8])> {
    let mut output = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    while cursor < data.len() {
        if !predicate(data[cursor]) {
            cursor += 1;
            continue;
        }
        let begin = cursor;
        while cursor < data.len() && predicate(data[cursor]) {
            cursor += 1;
        }
        if cursor - begin >= 20 {
            if begin > start {
                output.push((false, &data[start..begin]));
            }
            output.push((true, &data[begin..cursor]));
            start = cursor;
        }
    }
    if start < data.len() {
        output.push((false, &data[start..]));
    }
    output
}
pub fn source_segments(data: &str) -> Vec<QrSegment> {
    let bytes = data.as_bytes();
    if bytes.is_empty() {
        return Vec::new();
    }
    if bytes.len() <= 20 {
        return vec![
            if !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) {
                QrSegment::make_numeric(data)
            } else if !bytes.is_empty() && bytes.iter().all(|v| alpha(*v)) {
                QrSegment::make_alphanumeric(data)
            } else {
                QrSegment::make_bytes(bytes)
            },
        ];
    }
    let mut output = Vec::new();
    for (numeric, chunk) in split(bytes, |v| v.is_ascii_digit()) {
        if numeric {
            output.push(QrSegment::make_numeric(&String::from_utf8_lossy(chunk)));
        } else {
            for (alphanumeric, sub) in split(chunk, alpha) {
                output.push(if alphanumeric {
                    QrSegment::make_alphanumeric(&String::from_utf8_lossy(sub))
                } else {
                    QrSegment::make_bytes(sub)
                });
            }
        }
    }
    output
}
