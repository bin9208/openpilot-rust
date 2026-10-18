use super::{
    output::Output,
    placement::Placement,
    poll::{self, Poll},
    profile::{Reporter, RuntimeUsage},
    resources::Resources,
    stop::Stop,
    timing::Timing,
    Error, HelperConfig, Result,
};
use crate::{
    models::ClusterUiState,
    renderer::FrameClock,
    runtime_args::{Codec, Output as Mode, OutputRate, RuntimeArgs},
    runtime_preferences::Reader,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, Default, serde::Serialize)]
pub struct RunReport {
    pub frames: u64,
    pub last_state: Option<ClusterUiState>,
}
struct Loop {
    timing: Timing,
    polls: Poll,
    reporter: Reporter,
    usage: RuntimeUsage,
    report: RunReport,
    status_frames: u32,
    reported: f64,
}
impl Loop {
    fn new(args: &RuntimeArgs, now: f64, profile_started: f64) -> Self {
        Self {
            timing: Timing::new(now),
            polls: Poll::new(now),
            reporter: Reporter::new(
                args.diagnostics.profile_render,
                args.profile_interval,
                profile_started,
            ),
            usage: RuntimeUsage::default(),
            report: RunReport::default(),
            status_frames: 0,
            reported: now,
        }
    }
    fn frame(
        &mut self,
        args: &RuntimeArgs,
        reader: &Reader,
        resources: &mut Resources,
        output: &mut Output,
        rate: &mut OutputRate,
        placement: &mut Placement,
    ) -> Result<bool> {
        let started = Instant::now();
        resources.renderer.clear_profile_samples();
        output.check_error()?;
        self.reporter.samples(output.profile_samples()?);
        if args.output != Mode::Usb && resources.renderer.should_close() {
            return Ok(false);
        }
        let clock = FrameClock::now()?;
        let now = clock.perf_counter;
        if !self
            .polls
            .update(now, args, reader, resources, output, rate)?
            || self.timing.finished(now, args.duration)
        {
            return Ok(false);
        }
        let dt = self.timing.step(now);
        let camera = poll::camera(resources);
        let mut frame = resources
            .input
            .update(now, dt, &mut resources.renderer, camera)?;
        resources.active.camera_view_mode = frame.camera_view;
        let onroad = resources.input.onroad()?;
        if poll::live(args) {
            placement.update(onroad == Some(true), false, output.encoder_pid(), now)?;
        }
        if args.cluster_hud_mode.is_some() && onroad == Some(false) && reader.gate().mode == 0 {
            return Ok(false);
        }
        self.adorn(args, resources, &mut frame.state, now)?;
        if args.output != Mode::Usb {
            let start = Instant::now();
            resources
                .input
                .window_frame(&mut resources.renderer, &frame.state, clock)?;
            self.reporter.elapsed("main.window_render_total", start);
        }
        if args.usb_output() {
            let start = Instant::now();
            output.render(
                &mut resources.renderer,
                &frame.state,
                clock,
                resources.active.mirror,
            )?;
            self.reporter.elapsed(
                if args.usb_codec == Codec::H264 {
                    "main.usb_h264.output_total"
                } else {
                    "main.usb.output_total"
                },
                start,
            );
        }
        self.reporter
            .samples(resources.input.drain_profile_samples());
        self.reporter
            .samples(resources.renderer.profile_samples().iter().cloned());
        self.reporter.samples(output.profile_samples()?);
        self.report.frames = self.report.frames.saturating_add(1);
        self.status_frames = self.status_frames.saturating_add(1);
        self.reporter.elapsed("main.frame_active", started);
        self.sleep(rate.target_fps, started);
        let now = crate::live::monotonic()?;
        self.reporter.elapsed("main.frame_total", started);
        self.reporter.frame_done();
        self.status(now, &frame.state, frame.status.as_deref());
        self.report.last_state = Some(frame.state);
        Ok(true)
    }
    fn adorn(
        &mut self,
        args: &RuntimeArgs,
        resources: &mut Resources,
        state: &mut ClusterUiState,
        now: f64,
    ) -> Result<()> {
        state.radar_info_mode = resources.active.radar.info;
        state.radar_display_mode = resources.active.radar.display;
        state.radar_source_color_mode = resources.active.radar.color;
        state.camera_view_mode = poll::camera(resources);
        if !resources.active.clock_visible {
            state.center_clock_text = None;
        } else if poll::live(args) {
            state.center_clock_text = Some(chrono::Local::now().format("%H:%M:%S").to_string());
        }
        state.git_status = Some(resources.git.status(now)?);
        state.network_address = resources.network.address(now)?;
        state.network_connected = state.network_address.is_some();
        state.actual_fps = self.timing.actual_fps;
        let start = Instant::now();
        state.cluster_core_usage_text = resources
            .cores
            .as_mut()
            .map_or(Ok(None), |cores| cores.sample_text(now))?;
        if resources.cores.is_some() {
            self.reporter
                .elapsed("main.cluster_core_usage_sample", start);
        }
        Ok(())
    }
    fn sleep(&mut self, fps: f64, started: Instant) {
        if fps > 0.0 {
            let remaining = 1.0 / fps - started.elapsed().as_secs_f64();
            if remaining > 0.0 {
                let sleep = Instant::now();
                std::thread::sleep(Duration::from_secs_f64(remaining));
                self.reporter.elapsed("main.sleep", sleep);
            }
        }
    }
    fn status(&mut self, now: f64, state: &ClusterUiState, source: Option<&str>) {
        if self.reporter.ready(now) {
            let elapsed = self.reporter.elapsed_seconds(now);
            if let Some(report) = self.reporter.report(now, &self.usage.summary(elapsed)) {
                print!("{report}");
            }
        }
        if now - self.reported >= 2.0 {
            println!(
                "Refresh {:.1} Hz | speed={:5.1} km/h accel={:+.2} m/s^2 limit={}:{} gear={} cam={} {}",
                f64::from(self.status_frames) / (now - self.reported),
                state.speed_kph,
                state.accel_mps2,
                state
                    .speed_limit_kph
                    .map_or_else(|| "None".into(), |value| value.to_string()),
                state.speed_limit_source.as_deref().unwrap_or("-"),
                state.gear_text.as_deref().unwrap_or("-"),
                state.camera_view_mode,
                source.unwrap_or("")
            );
            self.reported = now;
            self.status_frames = 0;
        }
    }
}

/// # Errors
/// Returns input, render, codec, transport, signal or scheduling failures after owned cleanup.
pub fn run(root: &Path, args: &RuntimeArgs, helpers: &HelperConfig) -> Result<RunReport> {
    let board = Path::new("/TICI").is_file();
    let mut placement = Placement::new(board)?;
    let stop = Stop::install()?;
    let interrupt = openpilot_managed_entry::Sigint::install()?;
    let reader = Reader::for_runtime();
    let profile_started = crate::live::monotonic()?;
    let preferences = reader.snapshot();
    let mut rate = args.output_rate(board, preferences.usb_fps.to_bits() == 5.0_f64.to_bits())?;
    let opened = Output::open_with(args, &preferences, rate, |geometry| {
        Resources::new(root, args, preferences.clone(), geometry, rate, helpers)
    });
    let (mut output, mut resources) = match opened {
        Ok(opened) => opened,
        Err(Error::Stopped) => return Ok(RunReport::default()),
        Err(error) => return Err(error),
    };
    let mut state = Loop::new(args, resources.started, profile_started);
    openpilot_startup_ui::graphics_cleanup::trim_unused_heap();
    state
        .reporter
        .samples(resources.input.drain_profile_samples());
    state
        .reporter
        .samples(resources.renderer.profile_samples().iter().cloned());
    let result = (|| {
        while !stop.requested() && !interrupt.requested() {
            if !state.frame(
                args,
                &reader,
                &mut resources,
                &mut output,
                &mut rate,
                &mut placement,
            )? {
                break;
            }
        }
        Ok(())
    })();
    if stop.requested() {
        println!("Received signal 15; shutting down cluster HUD");
    }
    let placement = placement.update(false, true, output.encoder_pid(), crate::live::monotonic()?);
    let encoder_close = output.close_encoder();
    let resources_close = resources.close();
    let output_close = output.close();
    result
        .and(placement.map_err(Error::from))
        .and(encoder_close)
        .and(output_close)
        .and(resources_close)?;
    if interrupt.requested() {
        return Err(Error::Interrupted);
    }
    Ok(state.report)
}
