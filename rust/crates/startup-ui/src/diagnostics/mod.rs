//! Rust-owned application diagnostics, recording and presentation policy.
pub mod fps;
pub mod options;
mod overlay;
pub mod profile;
pub mod recording;
use crate::{geometry::MouseEvent, number, renderer::Renderer, Error};
pub use options::Options;
use recording::{Encoding, Recorder};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
pub struct Diagnostics {
    pub options: Options,
    pub profile: profile::Profile,
    pub frames: u64,
    pub record_dir: PathBuf,
    pub record_max_seconds: f64,
    pub record_every: u64,
    pub dynamic_recording: bool,
    recorder: Option<Recorder>,
    history: overlay::TouchHistory,
    target_fps: i32,
    fps_monitor: fps::Monitor,
    recording_started: f64,
    recording_frames: u64,
    profile_started: Instant,
}
impl Diagnostics {
    pub fn new(renderer: &mut Renderer, options: Options, target_fps: i32) -> Result<Self, Error> {
        if target_fps <= 0 {
            return Err(Error::Contract("target fps must be positive"));
        }
        if options.burn_in || options.record {
            renderer.ensure_render_target(scaled_dimensions(renderer)?)?;
        }
        if options.burn_in {
            renderer.burn_in()?;
        }
        let recorder = if options.record {
            Some(Recorder::start(
                &Encoding {
                    width: scaled_dimensions(renderer)?.0,
                    height: scaled_dimensions(renderer)?.1,
                    fps: target_fps,
                    speed: options.record_speed,
                    quality: options.record_quality,
                    bitrate: options.record_bitrate.clone(),
                    preset: "veryfast",
                    capacity: 60,
                },
                &options.record_output,
            )?)
        } else {
            None
        };
        Ok(Self {
            options,
            profile: profile::Profile::default(),
            frames: 0,
            record_dir: "/data/media/0/videos".into(),
            record_max_seconds: 60.0,
            record_every: 3,
            dynamic_recording: false,
            recorder,
            history: overlay::TouchHistory::default(),
            target_fps,
            fps_monitor: fps::Monitor::new(target_fps, monotonic_now()),
            recording_started: 0.0,
            recording_frames: 0,
            profile_started: Instant::now(),
        })
    }
    pub fn startup_profile(&self, elapsed: Duration) -> bool {
        if self.options.profile_startup {
            println!(
                "\n=== Startup profile ===\nRust window initialization: {:.3} ms\nUI window ready in {:.1} ms",
                elapsed.as_secs_f64() * 1000.0,
                elapsed.as_secs_f64() * 1000.0
            );
            true
        } else {
            false
        }
    }
    pub fn is_recording(&self) -> bool {
        self.dynamic_recording
    }
    pub fn recording_child_pid(&self) -> Option<u32> {
        self.recorder.as_ref().map(Recorder::child_pid)
    }
    pub fn start_recording(&mut self, renderer: &mut Renderer) -> Result<(), Error> {
        if self.dynamic_recording {
            return Ok(());
        }
        renderer.ensure_render_target((
            number::integer(renderer.dimensions().0)?,
            number::integer(renderer.dimensions().1)?,
        ))?;
        self.close_recording()?;
        std::fs::create_dir_all(&self.record_dir)?;
        let path = self.record_dir.join(format!(
            "{}.mp4",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        self.recorder = Some(Recorder::start(
            &Encoding {
                width: number::integer(renderer.dimensions().0)?,
                height: number::integer(renderer.dimensions().1)?,
                fps: self.target_fps,
                speed: 1,
                quality: 23,
                bitrate: String::new(),
                preset: "ultrafast",
                capacity: 8,
            },
            &path,
        )?);
        self.dynamic_recording = true;
        self.recording_started = monotonic_now();
        println!("[REC] start -> {}", path.display());
        Ok(())
    }
    pub fn stop_recording(&mut self) -> Result<(), Error> {
        if !self.dynamic_recording {
            return Ok(());
        }
        self.dynamic_recording = false;
        self.close_recording()?;
        println!("[REC] stop");
        Ok(())
    }
    pub fn toggle_recording(&mut self, renderer: &mut Renderer) -> Result<(), Error> {
        if self.dynamic_recording {
            self.stop_recording()
        } else {
            self.start_recording(renderer)
        }
    }
    pub fn close_recording(&mut self) -> Result<(), Error> {
        if let Some(mut recorder) = self.recorder.take() {
            recorder.close()?;
        }
        Ok(())
    }
    pub fn overlays(
        &mut self,
        renderer: &mut Renderer,
        events: &[MouseEvent],
    ) -> Result<(), Error> {
        if self.options.show_fps {
            renderer.draw_fps((10, 10));
        }
        let now = monotonic_now();
        if self.options.show_touches {
            self.history.draw(renderer, events, now)?;
        }
        overlay::grid(renderer, self.options.grid)?;
        Ok(())
    }
    /// Completes presentation after the caller has rendered its widgets.
    pub fn finish(
        &mut self,
        renderer: &mut Renderer,
        events: &[MouseEvent],
        started: Instant,
    ) -> Result<bool, Error> {
        if self.options.profile_frames > 0 {
            self.profile.record("render.content", started.elapsed());
        }
        let presentation = Instant::now();
        renderer.finish_content();
        let now = monotonic_now();
        self.overlays(renderer, events)?;
        renderer.present();
        self.profile
            .record("render.present", presentation.elapsed());
        if self.options.record || self.dynamic_recording {
            self.recording_frames = self.recording_frames.saturating_add(1);
            if self.record_every > 0 && self.recording_frames.is_multiple_of(self.record_every) {
                let bytes = renderer.capture_pixels()?;
                self.recorder
                    .as_mut()
                    .ok_or(Error::Contract("recording writer missing"))?
                    .submit(bytes)?;
            }
            if self.dynamic_recording && now - self.recording_started >= self.record_max_seconds {
                self.stop_recording()?;
                self.start_recording(renderer)?;
            }
        }
        let fps = renderer.fps();
        let decision = self.fps_monitor.observe(fps::Sample {
            fps,
            now,
            strict: self.options.strict,
        });
        if decision.warning {
            crate::logging::emit(
                openpilot_logging::record::Level::Warning,
                format!("FPS dropped below {}: {fps}", self.target_fps),
            );
        }
        if decision.critical {
            crate::logging::emit(
                openpilot_logging::record::Level::Error,
                format!("FPS dropped critically below {fps}. Shutting down UI."),
            );
            self.close_recording()?;
            return Err(Error::Contract("FPS dropped critically; shutting down UI"));
        }
        self.frames = self.frames.saturating_add(1);
        if self.options.profile_frames > 0 && self.frames >= self.options.profile_frames {
            let elapsed = self.profile_started.elapsed().as_secs_f64() * 1000.0;
            use num_traits::ToPrimitive;
            let average = elapsed
                / self
                    .frames
                    .to_f64()
                    .ok_or(Error::Contract("frame count overflow"))?;
            println!(
                "\n=== Render loop profile ===\n{}\nRendered {} frames in {elapsed:.1} ms\nAverage frame time: {average:.2} ms ({:.1} FPS)",
                self.profile.report(self.options.profile_stats),
                self.frames,
                1000.0 / average
            );
            return Ok(true);
        }
        Ok(false)
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        if let Err(error) = self.close_recording() {
            eprintln!("UI diagnostics close: {error}");
        }
    }
}
fn scaled_dimensions(renderer: &Renderer) -> Result<(i32, i32), Error> {
    let width = number::integer(renderer.dimensions().0 * renderer.config.scale)?;
    let height = number::integer(renderer.dimensions().1 * renderer.config.scale)?;
    Ok((width + width % 2, height + height % 2))
}
pub fn monotonic_now() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    use num_traits::ToPrimitive;
    now.tv_sec.to_f64().unwrap_or(0.0) + now.tv_nsec.to_f64().unwrap_or(0.0) / 1e9
}
