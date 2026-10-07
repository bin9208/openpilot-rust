use crate::{cache::TimedCache, context::Context, params::Read, state::messages};
use openpilot_startup_ui::{diagnostics::Diagnostics, renderer::Renderer};
use openpilot_ui_framework::Error;
use std::{cell::Cell, rc::Rc};
pub(super) struct Recording {
    context: Context,
    active: Rc<Cell<bool>>,
    cache: TimedCache<bool>,
    last_index: i32,
    pub rendered: bool,
}
impl Recording {
    pub fn active(&self) -> bool {
        self.active.get()
    }
    pub fn new(context: Context, active: Rc<Cell<bool>>) -> Self {
        Self {
            context,
            cache: TimedCache::new(active.get()),
            active,
            last_index: -1,
            rendered: false,
        }
    }
    fn sync(&mut self, requested: bool, diagnostics: &Diagnostics) -> Result<bool, Error> {
        let recording = diagnostics.is_recording();
        self.active.set(recording);
        if requested != recording {
            self.context
                .params
                .put_bool_nonblocking("ScreenRecord", recording)?;
            if self.context.big {
                self.cache
                    .store_pending(recording, (self.context.now_monotonic)());
            }
        }
        Ok(recording)
    }
    pub fn update(
        &mut self,
        diagnostics: &mut Diagnostics,
        renderer: &mut Renderer,
    ) -> Result<(), Error> {
        if !std::mem::take(&mut self.rendered) {
            return Ok(());
        }
        let requested = if self.context.big {
            let context = self.context.clone();
            *self.cache.refresh((context.now_monotonic)(), || {
                context.params.boolean("ScreenRecord")
            })
        } else {
            self.context.params.boolean("ScreenRecord")?
        };
        let started = self.context.ui.borrow().started;
        if requested && (!self.context.big || started) {
            diagnostics.start_recording(renderer)?;
        } else {
            diagnostics.stop_recording()?;
        }
        self.sync(requested, diagnostics)?;
        let command = {
            let sm = self.context.messages.borrow();
            let message = messages::carrot_man(&sm.state)?;
            (
                message.get_carrot_cmd_index(),
                message
                    .get_carrot_cmd()
                    .map_err(crate::Error::from)?
                    .to_str()
                    .map_err(crate::Error::from)?
                    .to_owned(),
                message
                    .get_carrot_arg()
                    .map_err(crate::Error::from)?
                    .to_str()
                    .map_err(crate::Error::from)?
                    .to_owned(),
            )
        };
        if command.0 == self.last_index || self.last_index == -1 {
            self.last_index = command.0;
            return Ok(());
        }
        self.last_index = command.0;
        openpilot_startup_ui::logging::emit(
            openpilot_logging::record::Level::Info,
            format!(
                "CarrotMan command received: {} {} (index {})",
                command.1, command.2, command.0
            ),
        );
        if !started {
            if !self.context.big {
                diagnostics.stop_recording()?;
                self.sync(requested, diagnostics)?;
            }
            return Ok(());
        }
        if command.1 != "RECORD" {
            return Ok(());
        }
        match command.2.to_uppercase().as_str() {
            "START" => diagnostics.start_recording(renderer)?,
            "STOP" => diagnostics.stop_recording()?,
            "TOGGLE" => diagnostics.toggle_recording(renderer)?,
            _ => {}
        }
        self.sync(requested, diagnostics)?;
        Ok(())
    }
}
