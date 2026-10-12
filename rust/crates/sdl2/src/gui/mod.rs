//! Render resources remain on the owning thread.
//! ```compile_fail
//! fn require_send<T: Send>() {}
//! require_send::<openpilot_sdl2::gui::Window>();
//! ```
mod bridge;
use bridge::ffi;
use std::{marker::PhantomData, path::Path, rc::Rc};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("SDL GUI: {0}")]
    Native(#[from] cxx::Exception),
    #[error("SDL GUI contract: {0}")]
    Contract(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug)]
pub struct Image {
    pub width: i32,
    pub height: i32,
    pub rgba: Vec<u8>,
}
impl Image {
    fn parse(image: ffi::Image) -> Result<Self> {
        let width =
            usize::try_from(image.width).map_err(|_| Error::Contract("negative image width"))?;
        let height =
            usize::try_from(image.height).map_err(|_| Error::Contract("negative image height"))?;
        if width.checked_mul(height).and_then(|s| s.checked_mul(4)) != Some(image.rgba.len()) {
            return Err(Error::Contract("image byte count mismatch"));
        }
        Ok(Self {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Quit,
    WindowEvent { event: u8, data1: i32, data2: i32 },
    KeyDown { key: i32, repeat: bool },
    MouseDown { x: i32, y: i32, button: u8 },
    MouseUp { x: i32, y: i32, button: u8 },
    MouseMove { x: i32, y: i32 },
    Other(u32),
}
pub struct Context {
    native: cxx::UniquePtr<ffi::Context>,
    thread: PhantomData<Rc<()>>,
}
impl Context {
    /// # Errors
    /// Returns the provider load or initialization error.
    pub fn load(sdl: &str, ttf: &str) -> Result<Self> {
        Ok(Self {
            native: ffi::load(sdl, ttf)?,
            thread: PhantomData,
        })
    }
    /// # Errors
    /// Returns SDL's window creation error or invalid dimensions.
    pub fn window(
        &self,
        title: &str,
        width: i32,
        height: i32,
        position: (i32, i32),
        resizable: bool,
    ) -> Result<Window> {
        Ok(Window {
            native: self
                .native
                .window(title, width, height, position.0, position.1, resizable)?,
            thread: PhantomData,
        })
    }
    /// # Errors
    /// Returns an invalid path or font provider error.
    pub fn font(&self, path: &Path, size: i32, bold: bool) -> Result<Font> {
        Ok(Font {
            native: self.native.font(
                path.to_str()
                    .ok_or(Error::Contract("font path is not UTF-8"))?,
                size,
                bold,
            )?,
            thread: PhantomData,
        })
    }
}
pub struct Window {
    native: cxx::UniquePtr<ffi::Window>,
    thread: PhantomData<Rc<()>>,
}
impl Window {
    #[must_use]
    pub fn size(&self) -> (i32, i32) {
        let size = self.native.size();
        (size.width, size.height)
    }
    #[must_use]
    pub fn events(&mut self) -> Vec<Event> {
        self.native
            .pin_mut()
            .events()
            .into_iter()
            .map(|e| match e.kind {
                0x100 => Event::Quit,
                0x200 => Event::WindowEvent {
                    event: e.button,
                    data1: e.x,
                    data2: e.y,
                },
                0x300 => Event::KeyDown {
                    key: e.key,
                    repeat: e.repeat,
                },
                0x400 => Event::MouseMove { x: e.x, y: e.y },
                0x401 => Event::MouseDown {
                    x: e.x,
                    y: e.y,
                    button: e.button,
                },
                0x402 => Event::MouseUp {
                    x: e.x,
                    y: e.y,
                    button: e.button,
                },
                value => Event::Other(value),
            })
            .collect()
    }
    /// # Errors
    /// Returns an invalid buffer or window surface error.
    pub fn present(&mut self, rgba: &[u8], width: i32, height: i32) -> Result<()> {
        Ok(self.native.pin_mut().present(rgba, width, height)?)
    }
    /// # Errors
    /// Returns the window surface read error.
    pub fn capture(&self) -> Result<Image> {
        Image::parse(self.native.capture()?)
    }
}
pub struct Font {
    native: cxx::UniquePtr<ffi::Font>,
    thread: PhantomData<Rc<()>>,
}
impl Font {
    /// # Errors
    /// Returns the UTF-8 glyph rendering or allocation error.
    pub fn render(&self, text: &str, color: [u8; 3]) -> Result<Image> {
        if text.contains('\0') {
            return Err(Error::Contract("font text contains NUL"));
        }
        Image::parse(self.native.render(text, color[0], color[1], color[2])?)
    }
}
pub struct Tk {
    native: cxx::UniquePtr<ffi::Tk>,
    thread: PhantomData<Rc<()>>,
}
impl Tk {
    /// # Errors
    /// Returns the Tcl/Tk provider or initialization error.
    pub fn load(tcl: &str, tk: &str) -> Result<Self> {
        let executable = std::env::current_exe()?;
        let executable = executable
            .to_str()
            .ok_or(Error::Contract("executable path is not UTF-8"))?;
        Ok(Self {
            native: ffi::load_tk(tcl, tk, executable)?,
            thread: PhantomData,
        })
    }
    /// # Errors
    /// Returns the interpreter command error. Arguments are passed as Tcl objects.
    pub fn eval(&mut self, words: &[String]) -> Result<String> {
        Ok(self.native.pin_mut().eval(words)?)
    }
}
