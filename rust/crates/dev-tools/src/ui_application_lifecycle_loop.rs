//! Independent application generator sequencing and desktop mouse transitions.
//! Drawing/input/encoder endpoints are mechanical; no candidate widgets or
//! captured scene data enter this controller.
use num_bigint::BigInt;
use serde::Serialize;
#[derive(Debug, PartialEq)]
pub enum LoopError {
    QueueFull,
    Interrupted,
    Failure(String),
}
#[derive(Clone, Copy, Debug)]
pub enum Event {
    MousePoll,
    MouseEvents,
    PollInput,
    Sleep(f64),
    Begin,
    TargetBegin,
    TargetEnd,
    Clear,
    MatrixPush,
    MatrixScale(f64),
    MatrixPop,
    Ticks,
    Widgets { width: f32, height: f32 },
    ShaderBegin,
    ShaderEnd,
    TargetDraw { width: f32, height: f32 },
    ShowFps,
    ShowTouches,
    Grid,
    End,
    RecordReadback,
    RecordSubmit,
    RecordUnload,
    StopRecording,
    StartRecording,
    MonitorFps,
    ProfileOutput(usize),
}
pub trait Backend {
    fn should_close(&mut self, frame: usize) -> Result<bool, LoopError>;
    fn close_requested(&self) -> bool {
        false
    }
    fn now(&self) -> f64;
    /// Dynamic recording start time and maximum duration; None when stopped.
    fn recording(&self) -> Option<(f64, f64)>;
    fn event(&mut self, event: Event) -> Result<(), LoopError>;
}
pub struct Config {
    pub pc: bool,
    pub width: f64,
    pub height: f64,
    pub scaled_width: f64,
    pub scaled_height: f64,
    pub fps: f64,
    pub scale: f64,
    pub texture: bool,
    pub texture_valid: bool,
    pub shader: bool,
    pub burn_in: bool,
    pub show_fps: bool,
    pub show_touches: bool,
    pub grid: bool,
    pub global_record: bool,
    pub profile_frames: BigInt,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            pc: true,
            width: 536.0,
            height: 240.0,
            scaled_width: 536.0,
            scaled_height: 240.0,
            fps: 20.0,
            scale: 1.0,
            texture: false,
            texture_valid: true,
            shader: true,
            burn_in: false,
            show_fps: false,
            show_touches: false,
            grid: false,
            global_record: false,
            profile_frames: 0.into(),
        }
    }
}
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Yield(bool),
    Stopped,
    ProfileComplete,
}
pub struct Loop {
    config: Config,
    frame: usize,
    record_frame: usize,
    pending: bool,
    finished: bool,
    should_render: bool,
    close: bool,
}
impl Loop {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            frame: 0,
            record_frame: 0,
            pending: false,
            finished: false,
            should_render: true,
            close: false,
        }
    }
    pub fn frame(&self) -> usize {
        self.frame
    }
    pub fn record_frame_index(&self) -> usize {
        self.record_frame
    }
    pub fn request_close(&mut self) {
        self.close = true;
    }
    pub fn set_should_render(&mut self, enabled: bool) {
        self.should_render = enabled;
    }
    pub fn resume(&mut self, backend: &mut dyn Backend) -> Result<Outcome, LoopError> {
        let result = self.advance(backend);
        match result {
            Err(LoopError::Interrupted) => {
                self.finished = true;
                Ok(Outcome::Stopped)
            }
            Err(error) => {
                self.finished = true;
                Err(error)
            }
            result => result,
        }
    }
    fn advance(&mut self, backend: &mut dyn Backend) -> Result<Outcome, LoopError> {
        if self.finished {
            return Ok(Outcome::Stopped);
        }
        // This phase belongs to the frame previously yielded to its consumer.
        // Dropping the controller after Yield executes no additional callbacks.
        if self.pending {
            self.pending = false;
            if self.config.scale != 1.0 {
                backend.event(Event::MatrixPop)?;
            }
            if self.config.texture {
                backend.event(Event::TargetEnd)?;
                backend.event(Event::Begin)?;
                backend.event(Event::Clear)?;
                if self.config.texture_valid {
                    let shader = self.config.burn_in && self.config.shader;
                    if shader {
                        backend.event(Event::ShaderBegin)?;
                    }
                    backend.event(Event::TargetDraw {
                        width: self.config.scaled_width as f32,
                        height: self.config.scaled_height as f32,
                    })?;
                    if shader {
                        backend.event(Event::ShaderEnd)?;
                    }
                }
            }
            if self.config.show_fps {
                backend.event(Event::ShowFps)?;
            }
            if self.config.show_touches {
                backend.event(Event::ShowTouches)?;
            }
            if self.config.grid {
                backend.event(Event::Grid)?;
            }
            backend.event(Event::End)?;
            if self.config.global_record || backend.recording().is_some() {
                self.record_frame += 1;
                if self.record_frame.is_multiple_of(3) {
                    backend.event(Event::RecordReadback)?;
                    match backend.event(Event::RecordSubmit) {
                        Ok(()) | Err(LoopError::QueueFull) => {}
                        Err(error) => return Err(error),
                    }
                    backend.event(Event::RecordUnload)?;
                }
                if let Some((started, duration)) = backend.recording() {
                    if backend.now() - started >= duration {
                        backend.event(Event::StopRecording)?;
                        backend.event(Event::StartRecording)?;
                    }
                }
            }
            backend.event(Event::MonitorFps)?;
            self.frame += 1;
            if self.config.profile_frames > BigInt::from(0)
                && BigInt::from(self.frame) >= self.config.profile_frames
            {
                backend.event(Event::ProfileOutput(self.frame))?;
                self.finished = true;
                return Ok(Outcome::ProfileComplete);
            }
        }
        if self.close || backend.close_requested() || backend.should_close(self.frame)? {
            self.finished = true;
            return Ok(Outcome::Stopped);
        }
        if self.config.pc {
            backend.event(Event::MousePoll)?;
        }
        backend.event(Event::MouseEvents)?;
        if !self.should_render {
            if self.config.pc {
                backend.event(Event::PollInput)?;
            }
            if self.config.fps == 0.0 {
                return Err(LoopError::Failure("division by zero".into()));
            }
            backend.event(Event::Sleep(1.0 / self.config.fps))?;
            return Ok(Outcome::Yield(false));
        }
        backend.event(if self.config.texture {
            Event::TargetBegin
        } else {
            Event::Begin
        })?;
        backend.event(Event::Clear)?;
        if self.config.scale != 1.0 {
            backend.event(Event::MatrixPush)?;
            backend.event(Event::MatrixScale(self.config.scale))?;
        }
        backend.event(Event::Ticks)?;
        backend.event(Event::Widgets {
            width: self.config.width as f32,
            height: self.config.height as f32,
        })?;
        self.pending = true;
        Ok(Outcome::Yield(true))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct MouseEvent {
    pub pos: Position,
    pub slot: usize,
    pub pressed: bool,
    pub released: bool,
    pub down: bool,
    pub time: f64,
}
pub struct Mouse {
    previous: [Option<MouseEvent>; 2],
    scale: f64,
}
impl Mouse {
    pub fn new(scale: f64) -> Self {
        Self {
            previous: [None, None],
            scale,
        }
    }
    pub fn poll(
        &mut self,
        down: [bool; 2],
        positions: [(f64, f64); 2],
        now: f64,
    ) -> Vec<MouseEvent> {
        let mut events = Vec::new();
        for slot in 0..2 {
            let previous = self.previous[slot];
            let was_down = previous.is_some_and(|event| event.down);
            let pos = if down[slot] {
                Position {
                    x: positions[slot].0 / self.scale,
                    y: positions[slot].1 / self.scale,
                }
            } else {
                previous.map_or(Position { x: 0.0, y: 0.0 }, |event| event.pos)
            };
            let event = MouseEvent {
                pos,
                slot,
                pressed: down[slot] && !was_down,
                released: was_down && !down[slot],
                down: down[slot],
                time: now,
            };
            let changed = previous.is_none_or(|previous| {
                previous.pos != event.pos
                    || previous.pressed != event.pressed
                    || previous.released != event.released
                    || previous.down != event.down
            });
            if changed {
                events.push(event);
                self.previous[slot] = Some(event);
            }
        }
        events
    }
}
