use super::{route::Route, Error, HelperConfig, Result};
use crate::{
    models::ClusterUiState,
    navi_simulator,
    renderer::{ClusterUiRenderer, FrameClock},
    runtime_args::{InputSource, RuntimeArgs, Theme},
    simulator::{gamepad::DualSenseSimulator, ClusterSimulator, RandomInputSource},
};
use std::{path::Path, time::Instant};

pub struct Frame {
    pub state: crate::models::ClusterUiState,
    pub status: Option<String>,
    pub camera_view: i64,
}
enum Source {
    Random {
        input: RandomInputSource,
        simulator: ClusterSimulator,
    },
    Gamepad {
        input: DualSenseSimulator,
        simulator: ClusterSimulator,
    },
    Live(Box<crate::live::Input>),
    Navi(Box<navi_simulator::Source>),
    Route(Box<Route>),
}
pub struct Input {
    source: Source,
    overlay: Option<navi_simulator::Source>,
    profile: bool,
    samples: Vec<(String, f64)>,
}
impl Input {
    /// # Errors
    /// Returns input provider, route loader, receiver or helper startup errors.
    pub fn new(_root: &Path, args: &RuntimeArgs, now: f64, helpers: &HelperConfig) -> Result<Self> {
        let early_overlay = if args.navi.navi_overlay && args.input == InputSource::Route {
            Some(navi_source(args)?)
        } else {
            None
        };
        let start = Instant::now();
        let source = match args.input {
            InputSource::Random => Source::Random {
                input: RandomInputSource::default(),
                simulator: ClusterSimulator::default(),
            },
            InputSource::Gamepad => Source::Gamepad {
                input: DualSenseSimulator::new(
                    i32::try_from(args.controller_index)
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                    now,
                )?,
                simulator: ClusterSimulator::default(),
            },
            InputSource::Live => {
                let mut live = crate::live::Input::new(
                    args.live.live_include_can && !args.live.live_no_can,
                    u64::try_from(args.live_timeout_ms)
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                )?;
                live.set_profile_enabled(args.diagnostics.profile_render);
                Source::Live(Box::new(live))
            }
            InputSource::Navi => Source::Navi(Box::new(navi_source(args)?)),
            InputSource::Route => Source::Route(Box::new(Route::new(args, helpers)?)),
        };
        let samples = if args.diagnostics.profile_render && args.input == InputSource::Route {
            vec![(
                "source.route_load_initial".into(),
                start.elapsed().as_secs_f64() * 1000.0,
            )]
        } else {
            Vec::new()
        };
        let overlay = if early_overlay.is_some() {
            early_overlay
        } else if args.navi.navi_overlay && args.input != InputSource::Navi {
            Some(navi_source(args)?)
        } else {
            None
        };
        Ok(Self {
            source,
            overlay,
            profile: args.diagnostics.profile_render,
            samples,
        })
    }
    /// # Errors
    /// Returns separate replay-tool startup errors after the caller's output gate allows startup.
    pub fn start(&mut self) -> Result<()> {
        if let Source::Route(route) = &mut self.source {
            route.start()?;
        }
        Ok(())
    }
    /// # Errors
    /// Returns source update, route control, renderer or navigation overlay errors.
    pub fn update(
        &mut self,
        now: f64,
        dt: f64,
        renderer: &mut ClusterUiRenderer,
        camera_view: i64,
    ) -> Result<Frame> {
        if let Source::Route(route) = &mut self.source {
            let start = Instant::now();
            let frame = route.update(now, renderer, camera_view)?;
            self.record("source.route_update", start);
            return self.apply_overlay(frame);
        }
        self.sample(now, dt, camera_view)
    }
    /// # Errors
    /// Returns source update errors; route input requires `update` with its renderer controls.
    pub fn sample(&mut self, now: f64, dt: f64, camera_view: i64) -> Result<Frame> {
        let start = Instant::now();
        let (state, status, profile_name) = match &mut self.source {
            Source::Random { input, simulator } => {
                let command = input.update(dt);
                let status = format!(
                    "random R2={:.2} L2={:.2} LSX={:+.2}",
                    command.throttle, command.brake, command.steering
                );
                (
                    simulator.update(&command, dt),
                    status,
                    "source.random_update",
                )
            }
            Source::Gamepad { input, simulator } => {
                let command = input.read_input(now)?;
                (
                    simulator.update(&command, dt),
                    input.status_text(),
                    "source.gamepad_update",
                )
            }
            Source::Live(input) => {
                let state = input.update()?;
                let status = input.status_text(
                    crate::live::monotonic()?,
                    state.radar_points.len(),
                    state.detected_vehicles.len(),
                )?;
                self.samples.extend(input.profile_samples());
                (state, status, "source.live_update")
            }
            Source::Navi(input) => (input.update()?, input.status_text()?, "source.navi_update"),
            Source::Route(_) => {
                return Err(Error::Invalid(
                    "route input requires renderer controls".into(),
                ))
            }
        };
        self.record(profile_name, start);
        self.apply_overlay(Frame {
            state,
            status: Some(status),
            camera_view,
        })
    }
    fn record(&mut self, name: &str, start: Instant) {
        if self.profile {
            self.samples
                .push((name.into(), start.elapsed().as_secs_f64() * 1000.0));
        }
    }
    fn apply_overlay(&mut self, mut frame: Frame) -> Result<Frame> {
        if let Some(overlay) = &mut self.overlay {
            let start = Instant::now();
            frame.state = navi_simulator::merge_overlay_state(&frame.state, &overlay.update()?);
            let status = overlay.status_text()?;
            frame.status = Some(
                frame
                    .status
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| status.clone(), |s| format!("{s} | {status}")),
            );
            self.record("source.navi_overlay_update", start);
        }
        Ok(frame)
    }
    #[must_use]
    pub fn drain_profile_samples(&mut self) -> Vec<(String, f64)> {
        std::mem::take(&mut self.samples)
    }
    pub fn configure_live(&mut self, screen_mode: i64, hud_debug: i64) {
        if let Source::Live(live) = &mut self.source {
            live.set_hud_debug_mode(hud_debug);
            live.set_debug_panels_enabled(screen_mode == 1, matches!(screen_mode, 3 | 4), false);
        }
    }
    /// # Errors
    /// Returns malformed device-state subscription errors.
    pub fn onroad(&self) -> Result<Option<bool>> {
        match &self.source {
            Source::Live(live) => Ok(live.onroad_state()?),
            _ => Ok(None),
        }
    }
    /// # Errors
    /// Returns malformed camera or device brightness errors.
    pub fn brightness(&self) -> Result<Option<i32>> {
        match &self.source {
            Source::Live(live) => Ok(live.brightness()?),
            _ => Ok(None),
        }
    }
    /// # Errors
    /// Returns drawing or replay control overlay errors.
    pub fn window_frame(
        &mut self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
    ) -> crate::renderer::Result<()> {
        match &self.source {
            Source::Route(route) => route.window_frame(renderer, state, clock),
            _ => renderer.render_frame_at(state, clock, None),
        }
    }
    /// # Errors
    /// Returns owned helper or receiver shutdown errors after attempting both sources.
    pub fn close(&mut self) -> Result<()> {
        let source = match &mut self.source {
            Source::Gamepad { input, .. } => {
                input.close();
                Ok(())
            }
            Source::Navi(input) => input.close().map_err(Error::from),
            Source::Route(route) => route.close(),
            Source::Live(live) => {
                if live.close() {
                    Ok(())
                } else {
                    Err(Error::Invalid(
                        "live media worker did not stop before its deadline".into(),
                    ))
                }
            }
            Source::Random { .. } => Ok(()),
        };
        let overlay = self
            .overlay
            .as_mut()
            .map_or(Ok(()), |overlay| overlay.close().map_err(Error::from));
        source.and(overlay)
    }
}
fn navi_source(args: &RuntimeArgs) -> Result<navi_simulator::Source> {
    Ok(navi_simulator::Source::new(navi_simulator::Config {
        host: args.navi_host.clone(),
        port: u16::try_from(args.navi_port).map_err(|e| Error::Invalid(e.to_string()))?,
        advertise_ip: args.navi_advertise_ip.clone(),
        beacon: !args.navi.navi_no_beacon,
        publish_cereal: args.navi.navi_publish_cereal,
        map_theme: match args.navi_map_theme {
            Theme::Auto => "auto",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
        .into(),
    })?)
}
impl Drop for Input {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("cluster input close: {error}");
        }
    }
}
#[cfg(test)]
mod tests;
