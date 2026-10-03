//! Owning native application loop, navigation and lifecycle from application.py.
use crate::{
    canvas::Canvas,
    geometry::{MouseEvent, Rect},
    keys::KeyboardInput,
    stack::NavigationStack,
    widget::{Frame, NavigationQueue, NavigationRequest, WidgetHandle},
    Error,
};
use openpilot_startup_ui::{
    config::Config,
    diagnostics::{monotonic_now, Diagnostics, Options},
    input::{BoardInput, Mouse},
    renderer::Renderer,
};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod config;
pub use config::ApplicationConfig;
mod ticks;
pub use ticks::{Tick, TickRegistry};
pub struct Application {
    // Input worker must stop before the GL surface is released.
    board_input: Option<BoardInput>,
    pub diagnostics: Diagnostics,
    pub canvas: Canvas,
    pub translations: Rc<RefCell<crate::multilang::Multilang>>,
    pub stack: NavigationStack,
    pub navigation: NavigationQueue,
    pub awake: bool,
    pub should_render: bool,
    target_fps: i32,
    mouse: Mouse,
    last_event: MouseEvent,
    pub ticks: TickRegistry,
    interrupt: Arc<AtomicBool>,
    signal: signal_hook::SigId,
    startup_exit: bool,
}
impl Application {
    pub fn new(config: ApplicationConfig) -> Result<Self, Error> {
        let started = Instant::now();
        let translation_root = config
            .assets
            .parent()
            .ok_or(Error::Contract("asset parent missing"))?
            .join("ui/translations");
        let translations = Rc::new(RefCell::new(
            crate::multilang::Multilang::new(&translation_root, Some(&config.language))
                .map_err(|error| Error::Io(std::io::Error::other(error)))?,
        ));
        let mut renderer = Renderer::new_window(
            openpilot_startup_ui::renderer::Window {
                config: config.graphics,
                dimensions: config
                    .dimensions
                    .unwrap_or((config.graphics.width(), config.graphics.height())),
                title: config.title.clone(),
                spinner: false,
                language: config.language,
            },
            &config.assets,
        )?;
        renderer.set_title(&config.title);
        renderer.set_target_fps(if std::env::var("OFFSCREEN").as_deref() == Ok("1") {
            0
        } else {
            config.fps
        });
        let diagnostics = Diagnostics::new(&mut renderer, config.diagnostics, config.fps)?;
        let startup_exit = diagnostics.startup_profile(started.elapsed());
        let board_input = if config.graphics.pc || startup_exit {
            None
        } else {
            Some(BoardInput::start(config.graphics.scale)?)
        };
        let interrupt = Arc::new(AtomicBool::new(false));
        let signal = signal_hook::flag::register(signal_hook::consts::SIGINT, interrupt.clone())?;
        let stack = NavigationStack {
            render_depth: if config.graphics.large_viewport { 1 } else { 2 },
            ..Default::default()
        };
        Ok(Self {
            board_input,
            diagnostics,
            canvas: Canvas::new(renderer, &config.assets),
            translations,
            stack,
            navigation: NavigationQueue::default(),
            awake: true,
            should_render: true,
            target_fps: config.fps,
            mouse: Mouse::default(),
            last_event: MouseEvent::default(),
            ticks: TickRegistry::default(),
            interrupt,
            signal,
            startup_exit,
        })
    }
    pub fn change_language(
        &mut self,
        language: &str,
        params: &openpilot_params::Params,
    ) -> Result<(), Error> {
        self.translations
            .borrow_mut()
            .change_language(language, params)
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        self.canvas
            .renderer
            .set_language(self.translations.borrow().language());
        Ok(())
    }
    pub fn target_fps(&self) -> i32 {
        self.target_fps
    }
    pub fn frame_count(&self) -> u64 {
        self.diagnostics.frames
    }
    pub fn request_close(&self) {
        self.interrupt.store(true, Ordering::Relaxed);
    }
    pub fn clear_widgets(&mut self) {
        while self.navigation.pop().is_some() {}
        self.stack.widgets.clear();
    }
    pub fn push(&self, widget: WidgetHandle) {
        self.navigation.push(NavigationRequest::Push(widget));
    }
    pub fn pop(&self) {
        self.navigation.push(NavigationRequest::Pop(None));
    }
    pub fn add_tick(&mut self, tick: Tick) {
        self.ticks.add(tick);
    }
    pub fn remove_tick(&mut self, tick: &Tick) {
        self.ticks.remove(tick);
    }
    pub fn is_closed(&self) -> bool {
        self.startup_exit
            || self.stack.close_requested
            || self.interrupt.load(Ordering::Relaxed)
            || self.canvas.renderer.should_close()
    }
    pub fn set_show_touches(&mut self, value: bool) {
        self.diagnostics.options.show_touches = value;
    }
    pub fn set_show_fps(&mut self, value: bool) {
        self.diagnostics.options.show_fps = value;
    }
    pub fn toggle_recording(&mut self) -> Result<(), Error> {
        self.diagnostics.toggle_recording(&mut self.canvas.renderer)
    }
    pub fn render(
        &mut self,
        mut paint: impl FnMut(&Frame<'_>, &mut Canvas) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        self.render_cycle(
            |_, _, _| Ok(()),
            |frame, canvas, _, _, rendered| {
                if rendered {
                    paint(frame, canvas)?;
                }
                Ok(())
            },
        )
    }
    pub fn render_cycle(
        &mut self,
        mut before: impl FnMut(&Frame<'_>, &mut NavigationStack, &mut Canvas) -> Result<(), Error>,
        mut after: impl FnMut(
            &Frame<'_>,
            &mut Canvas,
            &mut Diagnostics,
            &mut NavigationStack,
            bool,
        ) -> Result<(), Error>,
    ) -> Result<bool, Error> {
        if self.is_closed() {
            return Ok(false);
        }
        let now = monotonic_now();
        let mut events = match &self.board_input {
            Some(input) => input.drain()?,
            None => Vec::new(),
        };
        if self.canvas.renderer.config.pc {
            for slot in 0..2 {
                let (position, down) = self.canvas.renderer.sample(i32::from(slot));
                if let Some(event) = self.mouse.sample(slot, position, down, now) {
                    events.push(event);
                }
            }
        }
        if let Some(event) = events.last() {
            self.last_event = *event;
        }
        let native = self.canvas.renderer.keyboard();
        let keyboard = KeyboardInput {
            queued: RefCell::new(native.queued),
            characters: RefCell::new(native.characters),
            down: native.down,
            pressed: native.pressed,
        };
        let frame = Frame {
            index: self.diagnostics.frames,
            now: self.canvas.renderer.time(),
            monotonic: now,
            keyboard: &keyboard,
            navigation: &self.navigation,
            dt: f64::from(self.canvas.renderer.frame_time()),
            target_fps: f64::from(self.target_fps),
            awake: self.awake,
            events: &events,
            last_event: self.last_event,
            cursor: self.canvas.renderer.mouse_position(),
            wheel: f64::from(self.canvas.renderer.wheel()),
            show_touches: self.diagnostics.options.show_touches,
        };
        self.stack.process(&frame)?;
        if self.stack.close_requested {
            return Ok(false);
        }
        if !self.should_render {
            if self.canvas.renderer.config.pc {
                self.canvas.renderer.poll_input();
            }
            std::thread::sleep(Duration::from_secs_f64(1.0 / f64::from(self.target_fps)));
            after(
                &frame,
                &mut self.canvas,
                &mut self.diagnostics,
                &mut self.stack,
                false,
            )?;
            self.stack.process(&frame)?;
            return Ok(false);
        }
        let started = Instant::now();
        self.canvas.renderer.begin();
        self.ticks.run()?;
        before(&frame, &mut self.stack, &mut self.canvas)?;
        self.stack.process(&frame)?;
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: self.canvas.renderer.dimensions().0,
            height: self.canvas.renderer.dimensions().1,
        };
        self.stack.render(&frame, rect, &mut self.canvas)?;
        after(
            &frame,
            &mut self.canvas,
            &mut self.diagnostics,
            &mut self.stack,
            true,
        )?;
        self.stack.process(&frame)?;
        if self
            .diagnostics
            .finish(&mut self.canvas.renderer, &events, started)?
        {
            self.stack.close_requested = true;
        }
        Ok(true)
    }
    pub fn run(&mut self) -> Result<(), Error> {
        while !self.is_closed() {
            self.render(|_, _| Ok(()))?;
        }
        Ok(())
    }
}
impl Drop for Application {
    fn drop(&mut self) {
        self.board_input.take();
        self.clear_widgets();
        signal_hook::low_level::unregister(self.signal);
    }
}
