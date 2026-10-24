//! Inherited application controls and owned recording process for Forms.
use crate::{
    seal::Result,
    ui_forms_input::{hex, Canvas},
};
use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{sync_channel, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn integer(name: &str, default: &str) -> Result<String> {
    let value = std::env::var_os(name).unwrap_or_else(|| default.into());
    let value = value
        .to_str()
        .ok_or("forms environment integer encoding")?
        .trim();
    let (negative, body) = if let Some(body) = value.strip_prefix('-') {
        (true, body)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut digits = String::new();
    let mut previous = false;
    for ch in body.chars() {
        if ch == '_' && previous {
            previous = false;
            continue;
        }
        let digit = crate::argparse_scan::decimal_value(ch)
            .ok_or_else(|| format!("invalid Forms {name} integer"))?;
        digits.push(char::from(b'0' + digit));
        previous = true;
    }
    if !previous || digits.len() > 4300 {
        return Err(format!("invalid or overlong Forms {name} integer").into());
    }
    let magnitude = digits.trim_start_matches('0');
    Ok(if magnitude.is_empty() {
        "0".into()
    } else if negative {
        format!("-{magnitude}")
    } else {
        magnitude.into()
    })
}
fn output_path(value: OsString) -> Result<PathBuf> {
    let mut path = PathBuf::from(value);
    if path.components().next_back() == Some(std::path::Component::ParentDir) {
        path.pop();
        path.push("...mp4");
    } else {
        let mut name = path
            .file_name()
            .ok_or("Forms RECORD_OUTPUT has empty name")?
            .to_owned();
        if path.extension().is_some_and(OsStr::is_empty) {
            name.push(".mp4");
            path.set_file_name(name);
        } else {
            path.set_extension("mp4");
        }
    }
    Ok(path)
}
fn multiply_20(value: &str) -> Result<String> {
    let mut carry = 0u32;
    let mut digits = Vec::new();
    for ch in value.trim_start_matches('-').bytes().rev() {
        let n = u32::from(ch - b'0') * 20 + carry;
        digits.push(char::from(b'0' + (n % 10) as u8));
        carry = n / 10;
    }
    while carry != 0 {
        digits.push(char::from(b'0' + (carry % 10) as u8));
        carry /= 10;
    }
    digits.reverse();
    let value = format!(
        "{}{}",
        if value.starts_with('-') { "-" } else { "" },
        digits.into_iter().collect::<String>()
    );
    if value.trim_start_matches('-').len() > 4300 {
        return Err("Forms RECORD_SPEED output conversion limit".into());
    }
    Ok(value)
}
pub struct Environment {
    pub startup: bool,
    pub vsync: bool,
    pub burn_in: bool,
    pub record: bool,
    pub grid: String,
    pub show_fps: bool,
    pub strict: bool,
    render_limit: String,
    quality: String,
    speed: String,
    output: PathBuf,
    started: Instant,
}
impl Environment {
    pub fn read() -> Result<Self> {
        let grid = integer("GRID", "0")?;
        let render_limit = integer("PROFILE_RENDER", "0")?;
        let _stats = integer("PROFILE_STATS", "100")?;
        let quality = integer("RECORD_QUALITY", "23")?;
        let speed = integer("RECORD_SPEED", "1")?;
        let output =
            output_path(std::env::var_os("RECORD_OUTPUT").unwrap_or_else(|| "output".into()))?;
        let enabled = |name| std::env::var_os(name).as_deref() == Some(OsStr::new("1"));
        Ok(Self {
            startup: std::env::var_os("PROFILE_STARTUP").is_some(),
            vsync: enabled("ENABLE_VSYNC"),
            burn_in: std::env::var_os("BURN_IN").is_some(),
            record: enabled("RECORD"),
            grid,
            show_fps: enabled("SHOW_FPS"),
            strict: enabled("STRICT_MODE"),
            render_limit,
            quality,
            speed,
            output,
            started: Instant::now(),
        })
    }
    pub fn startup_complete(&self) -> bool {
        if !self.startup {
            return false;
        }
        println!(
            "\n=== Startup profile ===\nnative Forms window, render target, recorder and font initialization"
        );
        println!(
            "\x1b[92mUI window ready in {:.1} ms\x1b[0m",
            self.started.elapsed().as_secs_f64() * 1000.0
        );
        true
    }
    pub fn render_complete(&self, frames: usize) -> bool {
        let count = frames.to_string();
        let limit = &self.render_limit;
        if limit == "0"
            || limit.starts_with('-')
            || limit.len() > count.len()
            || (limit.len() == count.len() && limit > &count)
        {
            return false;
        }
        let elapsed = self.started.elapsed().as_secs_f64() * 1000.0;
        let average = elapsed / frames as f64;
        println!("\n=== Render loop profile ===\nnative Forms render loop");
        println!("\n\x1b[92mRendered {frames} frames in {elapsed:.1} ms\x1b[0m");
        println!(
            "\x1b[92mAverage frame time: {average:.2} ms ({:.1} FPS)\x1b[0m",
            1000.0 / average
        );
        true
    }
    pub fn start_recording(&self, width: i32, height: i32) -> Result<Option<Recorder>> {
        if !self.record {
            return Ok(None);
        }
        let mut args = [
            "-v", "warning", "-nostats", "-f", "rawvideo", "-pix_fmt", "rgba", "-s",
        ]
        .map(OsString::from)
        .to_vec();
        args.push(format!("{width}x{height}").into());
        args.extend(
            [
                "-r",
                "20",
                "-i",
                "pipe:0",
                "-vf",
                "vflip,format=yuv420p",
                "-r",
            ]
            .map(OsString::from),
        );
        args.push(multiply_20(&self.speed)?.into());
        args.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf"].map(OsString::from));
        args.push(self.quality.clone().into());
        if let Some(bitrate) = std::env::var_os("RECORD_BITRATE").filter(|v| !v.is_empty()) {
            for option in ["-b:v", "-maxrate", "-bufsize"] {
                args.push(option.into());
                args.push(bitrate.clone());
            }
        }
        args.extend(["-y", "-f", "mp4"].map(OsString::from));
        args.push(self.output.as_os_str().to_owned());
        Recorder::start(&args).map(Some)
    }
}
pub struct Recorder {
    child: Child,
    sender: Option<SyncSender<Vec<u8>>>,
    writer: Option<JoinHandle<()>>,
    frame: tempfile::NamedTempFile,
}
impl Recorder {
    fn start(args: &[OsString]) -> Result<Self> {
        let frame = tempfile::NamedTempFile::new()?;
        let mut child = Command::new("ffmpeg")
            .args(args)
            .stdin(Stdio::piped())
            .spawn()?;
        let mut input = child.stdin.take().ok_or("Forms recorder stdin missing")?;
        let (sender, receiver) = sync_channel::<Vec<u8>>(60);
        let writer = thread::spawn(move || {
            while let Ok(frame) = receiver.recv() {
                if input.write_all(&frame).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            sender: Some(sender),
            writer: Some(writer),
            frame,
        })
    }
    pub fn submit(&self, api: &mut dyn Canvas) -> Result<()> {
        api.call(&format!(
            "target-rgba {}",
            hex(self
                .frame
                .path()
                .to_str()
                .ok_or("Forms recording temp path UTF8")?)
        ))?;
        let data = std::fs::read(self.frame.path())?;
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(data);
        }
        Ok(())
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        drop(self.sender.take());
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.writer.as_ref().is_some_and(|v| !v.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if self.writer.as_ref().is_some_and(JoinHandle::is_finished) {
            let _ = self.writer.take().unwrap().join();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        if let Some(pid) = rustix::process::Pid::from_raw(self.child.id() as i32) {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
