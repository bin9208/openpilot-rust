use super::{
    catalog,
    media::{positive, remove, Media},
    Failure,
};
use crate::Value;
use std::{ffi::OsString, path::PathBuf};

impl Media {
    pub fn video(&self, segment: &Value) -> Result<(PathBuf, &'static str), Failure> {
        let directory = self.paths.segment_dir(segment)?;
        let (source, name) = catalog::source_video(&directory)?;
        if name.ends_with(".mp4") {
            return Ok((source, "video/mp4"));
        }
        let output = self.paths.cache_path("video", segment, ".mp4")?;
        if positive(&output) {
            return Ok((output, "video/mp4"));
        }
        for silent in [false, true] {
            let mut args = vec![
                OsString::from("-i"),
                source.clone().into_os_string(),
                OsString::from("-c"),
                OsString::from("copy"),
            ];
            if silent {
                args.push(OsString::from("-an"));
            }
            args.extend([
                OsString::from("-movflags"),
                OsString::from("+faststart"),
                output.clone().into_os_string(),
            ]);
            if self.run(&args, 180)?.success() && positive(&output) {
                return Ok((output, "video/mp4"));
            }
            remove(&output);
        }
        Ok((source, "video/mp2t"))
    }
}
