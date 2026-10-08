use openpilot_athena::{image, Error};
use openpilot_msgq::VisionMetadata;
use std::io::{self, BufRead};
fn main() -> Result<(), Error> {
    for line in io::stdin().lock().lines() {
        let row: serde_json::Value = serde_json::from_str(&line?)?;
        let value = |name| {
            row[name]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(Error::Contract("image field"))
        };
        let data: Vec<u8> = serde_json::from_value(row["data"].clone())?;
        let metadata = VisionMetadata {
            width: value("width")?,
            height: value("height")?,
            stride: value("stride")?,
            uv_offset: value("uv_offset")?,
            len: data.len(),
            frame_id: 80,
            timestamp_sof: 0,
            timestamp_eof: 0,
            valid: true,
            received: true,
            index: 0,
            fd: -1,
        };
        let rgb = image::extract(&data, &metadata)?;
        let jpeg = image::jpeg(&rgb, metadata.width, metadata.height)?;
        println!("{}", serde_json::json!({"rgb":rgb,"jpeg":jpeg}));
    }
    Ok(())
}
