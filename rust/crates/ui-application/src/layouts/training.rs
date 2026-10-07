//! Original big training page images and hit regions, with owned asynchronous decode.
use crate::{context::Context, paint};
use openpilot_startup_ui::renderer::DecodedImage;
use openpilot_ui_framework::{
    callback::Callback,
    draw::{Draw, ImageDraw, PixelBuffer, TextureResource, WHITE},
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use std::{
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
};
const fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
pub const STEP_RECTS: [Rect; 19] = [
    rect(104.0, 800.0, 633.0, 175.0),
    rect(1835.0, 0.0, 2159.0, 1080.0),
    rect(1835.0, 0.0, 2156.0, 1080.0),
    rect(1526.0, 473.0, 427.0, 472.0),
    rect(1643.0, 441.0, 217.0, 223.0),
    rect(1835.0, 0.0, 2155.0, 1080.0),
    rect(1786.0, 591.0, 267.0, 236.0),
    rect(1353.0, 0.0, 804.0, 1080.0),
    rect(1458.0, 485.0, 633.0, 211.0),
    rect(95.0, 794.0, 1158.0, 187.0),
    rect(1560.0, 170.0, 392.0, 397.0),
    rect(1835.0, 0.0, 2159.0, 1080.0),
    rect(1351.0, 0.0, 807.0, 1080.0),
    rect(1835.0, 0.0, 2158.0, 1080.0),
    rect(1531.0, 82.0, 441.0, 920.0),
    rect(1336.0, 438.0, 490.0, 393.0),
    rect(1835.0, 0.0, 2159.0, 1080.0),
    rect(1835.0, 0.0, 2159.0, 1080.0),
    rect(87.0, 795.0, 1187.0, 186.0),
];
const RECORD_YES: Rect = rect(695.0, 794.0, 558.0, 187.0);
const RESTART: Rect = rect(87.0, 795.0, 472.0, 186.0);
pub struct Training {
    pub state: WidgetState,
    context: Context,
    pub step: usize,
    textures: Vec<Box<dyn TextureResource>>,
    receiver: Option<Receiver<Result<DecodedImage, String>>>,
    worker: Option<JoinHandle<()>>,
    pub completed: Option<Callback<()>>,
}
impl Training {
    pub fn new(context: Context, draw: &mut dyn Draw) -> Result<Self, Error> {
        let directory = context
            .source_root
            .join("openpilot/selfdrive/assets/training");
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some(digits) = name
                .strip_prefix("step")
                .and_then(|s| s.strip_suffix(".png"))
            {
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                    let number = digits
                        .parse::<u32>()
                        .map_err(|_| Error::Contract("training image index overflow"))?;
                    paths.push((number, path));
                }
            }
        }
        paths.sort_by_key(|(number, _)| *number);
        if paths.len() != STEP_RECTS.len() {
            return Err(Error::Contract(
                "training image count differs from hit regions",
            ));
        }
        let mut paths = paths.into_iter().map(|(_, path)| path);
        let first = DecodedImage::load(
            &paths
                .next()
                .ok_or(Error::Contract("training first image missing"))?,
        )?;
        let first = draw.upload_image(PixelBuffer {
            dimensions: (first.width, first.height),
            rgba: &first.rgba,
        })?;
        let (sender, receiver) = mpsc::sync_channel(18);
        let worker = thread::Builder::new()
            .name("ui-training-decode".into())
            .spawn(move || {
                for path in paths {
                    let result = DecodedImage::load(&path).map_err(|e| e.to_string());
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            state: WidgetState::default(),
            context,
            step: 0,
            textures: vec![first],
            receiver: Some(receiver),
            worker: Some(worker),
            completed: None,
        })
    }
    pub fn uploaded(&self) -> usize {
        self.textures.len()
    }
    pub fn finish_decode(&mut self) -> Result<(), Error> {
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Contract("training decoder panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Training {
    fn drop(&mut self) {
        drop(self.receiver.take());
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("training decoder panicked during shutdown");
            }
        }
    }
}
impl Widget for Training {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(Ok(image)) => self.textures.push(draw.upload_image(PixelBuffer {
                    dimensions: (image.width, image.height),
                    rgba: &image.rgba,
                })?),
                Ok(Err(error)) => return Err(Error::Io(std::io::Error::other(error))),
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {}
            }
        }
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        if STEP_RECTS[self.step].contains(position) {
            if self.step == 9 {
                self.context
                    .params
                    .put_bool("RecordFront", RECORD_YES.contains(position))?;
            }
            if self.step == STEP_RECTS.len() - 1 && RESTART.contains(position) {
                self.step = 0;
            } else {
                self.step += 1;
            }
            if self.step >= STEP_RECTS.len() {
                self.step = 0;
                if let Some(callback) = &self.completed {
                    callback.call(());
                }
                frame.navigation.push(NavigationRequest::Pop(None));
            }
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let step = self.step.min(self.textures.len() - 1);
        let texture = &self.textures[step];
        let (width, height) = texture.dimensions();
        let image = rect(0.0, 0.0, float(f64::from(width)), float(f64::from(height)));
        draw.image(ImageDraw {
            id: texture.id(),
            source: image,
            destination: image,
            origin: Point::default(),
            rotation: 0.0,
            tint: WHITE,
        })?;
        if step > 0 && step < STEP_RECTS.len() - 1 {
            let step = f64::from(
                u32::try_from(step).map_err(|_| Error::Contract("training step overflow"))?,
            );
            draw.rounded(
                rect(
                    self.state.rect.x.trunc(),
                    (self.state.rect.y + self.state.rect.height - 20.0).trunc(),
                    float((step / 18.0 * f64::from(self.state.rect.width)).trunc()),
                    20.0,
                ),
                0.0,
                paint::color(70, 91, 234, 255),
            )?;
        }
        Ok(RenderResult::Value(-1))
    }
}
