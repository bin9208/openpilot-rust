mod actions;
pub mod hardware;
mod native;
use crate::{
    context::Context,
    device,
    root_layout::Main,
    scheduling,
    settings::resources::Resources,
    state::{self, messages},
};
use openpilot_ui_framework::{
    application::Application,
    callback::Callback,
    widget::{NavigationRequest, WidgetHandle},
    Error,
};
use std::{cell::Cell, path::PathBuf, rc::Rc, time::Duration};
pub struct Runtime {
    pub context: Context,
    pub root: WidgetHandle,
    pub onboarding: WidgetHandle,
    pub recording: Rc<Cell<bool>>,
    hardware: Box<dyn hardware::Hardware>,
    scheduler: scheduling::Scheduler,
    platform: scheduling::Native,
    effects: actions::Effects,
    prime: Option<crate::services::polling::Poller>,
    network: openpilot_ui_framework::network::WifiSession,
    pub app: Application,
}
impl Runtime {
    pub fn new(
        mut app: Application,
        context: Context,
        resources: Resources,
        hardware: Box<dyn hardware::Hardware>,
        camera: &str,
        updater: PathBuf,
    ) -> Result<Self, Error> {
        let recording = Rc::new(Cell::new(app.diagnostics.is_recording()));
        let root = WidgetHandle::new(Main::new(
            context.clone(),
            &mut app.canvas,
            resources.clone(),
            recording.clone(),
            camera,
        )?);
        app.push(root.clone());
        let (onboarding, completed) = if context.big {
            let onboarding = crate::layouts::onboarding::Onboarding::new(context.clone())?;
            let completed = onboarding.completed();
            (WidgetHandle::new(onboarding), completed)
        } else {
            let queue = app.navigation.clone();
            let target = root.downgrade();
            let completed = Rc::new(move || {
                if let Some(target) = target.upgrade() {
                    queue.push(NavigationRequest::PopTo {
                        target,
                        instant: false,
                        callback: None,
                    });
                }
            });
            let preview = crate::mici::onroad::driver_camera::Preview::with_camera(
                context.clone(),
                &mut app.canvas,
                camera,
                20.0,
                true,
            )?;
            let onboarding = crate::mici::layouts::onboarding::Onboarding::with_preview(
                context.clone(),
                &mut app.canvas,
                completed,
                preview,
            )?;
            let completed = onboarding.completed();
            (WidgetHandle::new(onboarding), completed)
        };
        if !completed {
            app.push(onboarding.clone());
        }
        let board = !context.pc;
        let mut runtime = Self {
            context: context.clone(),
            root,
            onboarding,
            recording,
            hardware,
            scheduler: scheduling::Scheduler::new(board),
            platform: scheduling::Native { root: "/".into() },
            effects: actions::Effects::new(
                context.clone(),
                updater,
                camera.into(),
                app.diagnostics.options.show_touches,
                app.diagnostics.options.show_fps,
            )?,
            prime: None,
            network: resources.network.session,
            app,
        };
        runtime.scheduler.update(
            scheduling::Request {
                onroad: false,
                force: true,
                child_pid: None,
                now: (context.now_monotonic)(),
            },
            &mut runtime.platform,
        )?;
        Ok(runtime)
    }
    pub fn step(&mut self) -> Result<bool, Error> {
        let Self {
            app,
            context,
            root,
            onboarding,
            recording,
            hardware,
            scheduler,
            platform,
            effects,
            prime,
            network,
        } = self;
        let rendered = app.render_cycle(
            |frame, stack, _| root.get_mut::<Main>()?.tick(root, onboarding, stack, frame),
            |frame, canvas, diagnostics, stack, rendered| {
                if rendered {
                    root.get_mut::<Main>()?
                        .finish_render(diagnostics, &mut canvas.renderer)?;
                }
                effects.drain(root, frame, canvas, diagnostics, stack, hardware.as_mut())?;
                recording.set(diagnostics.is_recording());
                if prime.is_none() {
                    let service = crate::services::prime::Prime::new(
                        context.prime.clone(),
                        context.params.raw.clone(),
                        context.api.clone(),
                    )
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                    *prime = Some(service.start(context.poll_gate.clone())?);
                }
                context
                    .messages
                    .borrow_mut()
                    .update(Duration::ZERO)
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                let input = state::Input::from_messages(
                    &context.messages.borrow().state,
                    messages::Clock {
                        now: (context.now_monotonic)(),
                        fps: canvas.renderer.fps(),
                    },
                )?;
                let models = {
                    let ui = context.ui.borrow();
                    if input.now - ui.param_update_time > 5.0 {
                        (context.model_status)()?
                    } else {
                        state::ModelStatus {
                            compiled: ui.slow.usbgpu_compiled,
                            compile_pending: ui.slow.usbgpu_compile_pending,
                        }
                    }
                };
                let transitions =
                    context
                        .ui
                        .borrow_mut()
                        .update(&input, context.params.as_ref(), models)?;
                for transition in transitions {
                    context.transition(transition);
                }
                let values = context.device.borrow_mut().update(
                    &context.ui.borrow(),
                    device::Input {
                        now: (context.now_monotonic)(),
                        left_down: frame.events.iter().any(|event| event.down),
                        brightness_worker_busy: hardware.brightness_busy(),
                        exposure_percent: input.exposure_percent,
                    },
                );
                for effect in values {
                    match effect {
                        device::Effect::Brightness(value) => hardware.brightness(value)?,
                        device::Effect::DisplayPower(value) => hardware.display_power(value)?,
                        device::Effect::InteractiveTimeout => {
                            context.event(crate::context::Event::InteractiveTimeout)
                        }
                    }
                }
                root.get_mut::<Main>()?
                    .events(root, onboarding, stack, frame)?;
                effects.drain(root, frame, canvas, diagnostics, stack, hardware.as_mut())?;
                context.sync_services();
                network.process()?;
                scheduler.update(
                    scheduling::Request {
                        onroad: context.ui.borrow().started,
                        force: false,
                        child_pid: diagnostics
                            .recording_child_pid()
                            .map(i32::try_from)
                            .transpose()
                            .map_err(|_| Error::Contract("recording child pid overflow"))?,
                        now: (context.now_monotonic)(),
                    },
                    platform,
                )?;
                Ok(())
            },
        )?;
        app.awake = context.device.borrow().awake;
        app.should_render = app.awake;
        app.set_show_touches(effects.show_touches);
        app.set_show_fps(effects.show_fps);
        Ok(rendered)
    }
    pub fn run(&mut self) -> Result<(), Error> {
        let result = (|| {
            while !self.app.is_closed() {
                self.step()?;
            }
            Ok(())
        })();
        let close = self.close();
        result.and(close)
    }
    pub fn close(&mut self) -> Result<(), Error> {
        self.context.poll_gate.update(true, false);
        self.prime.take();
        self.app.request_close();
        self.app.clear_widgets();
        self.app.diagnostics.stop_recording()?;
        self.context.params.flush()?;
        self.context.memory.flush()?;
        Ok(())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            openpilot_startup_ui::logging::emit(
                openpilot_logging::record::Level::Error,
                format!("UI close failed: {error}"),
            );
        }
    }
}
