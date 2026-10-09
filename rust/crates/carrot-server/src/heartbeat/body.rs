use crate::mapbox_tokens::body_error;
use std::io;

pub(crate) fn failure(error: io::Error, body: &ureq::Body, received: usize) -> String {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        if let Some(length) = body.raw_content_length() {
            return format!(
                "IncompleteRead({received} bytes read, {} more expected)",
                length.saturating_sub(u64::try_from(received).unwrap_or(u64::MAX))
            );
        }
        if let Some(completed) = body.completed_raw_chunk_bytes() {
            return format!("IncompleteRead({completed} bytes read)");
        }
    }
    body_error(error)
}
