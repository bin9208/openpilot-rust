use crate::diagnostics;
use crate::Error;
use openpilot_cereal::log_capnp::{event, sentinel::SentinelType};
use openpilot_logging::{log_site, record::Level};
use openpilot_params::Params;
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

struct Segment {
    path: PathBuf,
    rlog: zstd::stream::write::Encoder<'static, File>,
    qlog: zstd::stream::write::Encoder<'static, File>,
}

impl Segment {
    fn open(path: PathBuf) -> Result<Self, Error> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o775)
            .create(&path)?;
        File::create(path.join("rlog.lock"))?;
        Ok(Self {
            rlog: zstd::stream::write::Encoder::new(File::create(path.join("rlog.zst"))?, 10)?,
            qlog: zstd::stream::write::Encoder::new(File::create(path.join("qlog.zst"))?, 10)?,
            path,
        })
    }

    fn write(&mut self, bytes: &[u8], qlog: bool) -> Result<(), Error> {
        self.rlog.write_all(bytes)?;
        if qlog {
            self.qlog.write_all(bytes)?;
        }
        Ok(())
    }

    fn finish(self) -> Result<(), Error> {
        crate::files::close(self.rlog.finish()?)?;
        crate::files::close(self.qlog.finish()?)?;
        fs::remove_file(self.path.join("rlog.lock"))?;
        Ok(())
    }
}

pub struct Logger {
    root: PathBuf,
    pub route: String,
    pub part: i32,
    init: Vec<u8>,
    segment: Option<Segment>,
    preserved: Option<i32>,
}

impl Logger {
    pub fn new(root: &Path, route: String, init: Vec<u8>) -> Self {
        Self {
            root: root.to_owned(),
            route,
            init,
            part: -1,
            segment: None,
            preserved: None,
        }
    }

    pub fn path(&self) -> Result<&Path, Error> {
        self.segment
            .as_ref()
            .map(|segment| segment.path.as_path())
            .ok_or(Error::Invalid("segment is closed"))
    }

    pub fn next(&mut self, now: u64) -> Result<(), Error> {
        if self.segment.is_some() {
            self.sentinel(SentinelType::EndOfSegment, 0, now)?;
            if let Some(segment) = self.segment.take() {
                segment.finish()?;
            }
        }
        self.part = self
            .part
            .checked_add(1)
            .ok_or(Error::Invalid("segment overflow"))?;
        let mut segment = Segment::open(self.root.join(format!("{}--{}", self.route, self.part)))?;
        segment.write(&self.init, true)?;
        self.segment = Some(segment);
        self.sentinel(
            if self.part == 0 {
                SentinelType::StartOfRoute
            } else {
                SentinelType::StartOfSegment
            },
            0,
            now,
        )
    }

    pub fn write(&mut self, bytes: &[u8], qlog: bool) -> Result<(), Error> {
        self.segment
            .as_mut()
            .ok_or(Error::Invalid("segment is closed"))?
            .write(bytes, qlog)
    }

    fn sentinel(&mut self, kind: SentinelType, signal: i32, now: u64) -> Result<(), Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_valid(true);
        event.set_log_mono_time(now);
        let mut sentinel = event.init_sentinel();
        sentinel.set_type(kind);
        sentinel.set_signal(signal);
        self.write(&capnp::serialize::write_message_to_words(&message), true)
    }

    pub fn close(&mut self, signal: i32, now: u64) -> Result<(), Error> {
        if self.segment.is_some() {
            self.sentinel(SentinelType::EndOfRoute, signal, now)?;
            if let Some(segment) = self.segment.take() {
                segment.finish()?;
            }
        }
        Ok(())
    }

    pub fn preserve(&mut self, params: &Params) -> Result<(), Error> {
        if self.preserved == Some(self.part) {
            return Ok(());
        }
        diagnostics::emit(
            log_site!(),
            Level::Warning,
            format!("preserving {}", self.path()?.display()),
        );
        if let Err(error) = rustix::fs::setxattr(
            self.path()?,
            "user.preserve",
            b"1",
            rustix::fs::XattrFlags::empty(),
        ) {
            diagnostics::emit(
                log_site!(),
                Level::Error,
                format!(
                    "setxattr user.preserve failed for {}: {}",
                    self.path()?.display(),
                    crate::diagnostics::errno_text(error.raw_os_error())
                ),
            );
        }
        let mut routes = params
            .get("AthenadRecentlyViewedRoutes")?
            .unwrap_or_default();
        routes.push(b',');
        routes.extend_from_slice(self.route.as_bytes());
        params.put("AthenadRecentlyViewedRoutes", &routes)?;
        self.preserved = Some(self.part);
        Ok(())
    }
}

pub fn route_name(params: &Params) -> Result<String, Error> {
    identifier(params, "RouteCount")
}

pub fn identifier(params: &Params, key: &str) -> Result<String, Error> {
    let count = match params.get(key) {
        Ok(value) => value.unwrap_or_default(),
        Err(openpilot_params::Error::Io(_)) => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let value = String::from_utf8_lossy(&count);
    let value = value.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let negative = value.starts_with('-');
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    let magnitude = digits.parse::<u64>().unwrap_or(0);
    let counter = if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
    .to_le_bytes();
    let counter = u32::from_le_bytes([counter[0], counter[1], counter[2], counter[3]]);
    // logger_get_identifier ignores Params::put's I/O return code.
    let _ = params.put(key, counter.wrapping_add(1).to_string().as_bytes());
    let mut random = [0_u8; 5];
    getrandom::fill(&mut random)?;
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!("{counter:08x}--{suffix}"))
}
