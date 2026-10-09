use super::{
    catalog,
    media::{positive, remove, Media},
    Failure,
};
use crate::Value;
use std::{ffi::OsString, fs, path::PathBuf};

fn strings(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}
impl Media {
    fn placeholder(&self, segment: &Value) -> Result<PathBuf, Failure> {
        let output = self.paths.cache_path("placeholder", segment, ".svg")?;
        if positive(&output) {
            return Ok(output);
        }
        fs::write(&output, include_str!("media_placeholder.svg"))
            .map_err(|error| crate::state::io_error(error, &output))?;
        Ok(output)
    }
    pub fn thumbnail(&self, segment: &Value) -> Result<PathBuf, Failure> {
        let directory = self.paths.segment_dir(segment)?;
        let (source, _) = catalog::source_video(&directory)?;
        let output = self.paths.cache_path("thumb", segment, ".jpg")?;
        if positive(&output) {
            return Ok(output);
        }
        for seek in ["2", "0.2"] {
            let mut args = strings(&["-ss", seek, "-i"]);
            args.push(source.clone().into_os_string());
            args.extend(strings(&["-vframes", "1", "-vf", "scale=640:-1"]));
            args.push(output.clone().into_os_string());
            if self.run(&args, 90)?.success() && positive(&output) {
                return Ok(output);
            }
            remove(&output);
        }
        self.placeholder(segment)
    }
    pub fn preview(&self, segment: &Value) -> Result<PathBuf, Failure> {
        let directory = self.paths.segment_dir(segment)?;
        let (source, _) = catalog::source_video(&directory)?;
        let output = self.paths.cache_path("preview", segment, ".gif")?;
        if positive(&output) {
            return Ok(output);
        }
        let mut args = strings(&["-ss", "1", "-t", "2.4", "-i"]);
        args.push(source.into_os_string());
        args.extend(strings(&[
            "-vf",
            "fps=4,scale=360:-1:flags=lanczos",
            "-loop",
            "0",
        ]));
        args.push(output.clone().into_os_string());
        if !self.run(&args, 120)?.success() || !positive(&output) {
            remove(&output);
            return self.thumbnail(segment);
        }
        Ok(output)
    }
}
