#[cxx::bridge(namespace = "openpilot::sdlgui")]
pub(crate) mod ffi {
    struct Event {
        kind: u32,
        key: i32,
        x: i32,
        y: i32,
        button: u8,
        repeat: bool,
    }
    struct Size {
        width: i32,
        height: i32,
    }
    struct Image {
        width: i32,
        height: i32,
        rgba: Vec<u8>,
    }
    // SAFETY: CXX checks the primitive layouts; C++ resources own their loaded libraries.
    unsafe extern "C++" {
        include!("sdl2/gui.hpp");
        type Context;
        type Window;
        type Font;
        type Tk;
        fn load(sdl: &str, ttf: &str) -> Result<UniquePtr<Context>>;
        fn window(
            self: &Context,
            title: &str,
            width: i32,
            height: i32,
            x: i32,
            y: i32,
            resizable: bool,
        ) -> Result<UniquePtr<Window>>;
        fn font(self: &Context, path: &str, size: i32, bold: bool) -> Result<UniquePtr<Font>>;
        fn size(self: &Window) -> Size;
        fn events(self: Pin<&mut Window>) -> Vec<Event>;
        fn present(self: Pin<&mut Window>, rgba: &[u8], width: i32, height: i32) -> Result<()>;
        fn capture(self: &Window) -> Result<Image>;
        fn render(self: &Font, text: &str, r: u8, g: u8, b: u8) -> Result<Image>;
        fn load_tk(tcl: &str, tk: &str, executable: &str) -> Result<UniquePtr<Tk>>;
        fn eval(self: Pin<&mut Tk>, words: &[String]) -> Result<String>;
    }
}
