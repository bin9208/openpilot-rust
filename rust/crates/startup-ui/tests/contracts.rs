use openpilot_startup_ui::{
    children::{self, Child, Launch, StopError, Window},
    config::Config,
    geometry::{MouseEvent, Point, Rect},
    input::{Mouse, TouchSlots},
    network::IpState,
    scroll::Scroll,
    spinner::Spinner,
    text::{self, Font, Measure},
    Error,
};
use std::{cell::RefCell, rc::Rc, time::Duration};
struct Fixed;
impl Measure for Fixed {
    fn measure(&self, _: Font, text: &str, size: f32, _: f32) -> Point {
        Point {
            x: text.chars().map(|_| size).sum(),
            y: size,
        }
    }
}
fn config() -> Config {
    Config {
        big: false,
        large_viewport: false,
        pc: true,
        scale: 1.0,
    }
}
#[test]
fn spinner_unicode_numeric_and_sticky_status() {
    let mut s = Spinner::default();
    s.set_text("  compiling  ", config(), &Fixed).unwrap();
    assert_eq!(s.status, "compiling");
    s.set_text("٤٢", config(), &Fixed).unwrap();
    assert_eq!(s.progress, Some(42));
    assert!(s.lines.is_empty());
    s.set_text("next stage", config(), &Fixed).unwrap();
    assert_eq!(s.progress, Some(42));
    assert_eq!(s.status, "next stage");
    s.set_text("999999", config(), &Fixed).unwrap();
    assert_eq!(s.progress, Some(100));
    assert!(s.set_text("²", config(), &Fixed).is_err());
}
#[test]
fn text_wrap_keeps_source_indentation_and_empty_lines() {
    assert_eq!(
        text::wrap("\n abc def\n\n", 1.0, 5.0, &Fixed),
        [" abc ", "def", "", ""]
    );
    assert_eq!(text::strip_emoji("a😀😃b🚗c"), ("abc".into(), 2));
}
#[test]
fn ip_grace_retains_two_failures_then_clears() {
    let mut state = IpState::default();
    assert_eq!(state.label(6999), "no network");
    state.refresh(Some("192.0.2.5".into()));
    for _ in 0..2 {
        state.refresh(None);
        assert_eq!(state.label(6999), "192.0.2.5:6999");
    }
    state.refresh(None);
    assert_eq!(state.label(6999), "no network");
    state.refresh(Some("192.0.2.6".into()));
    assert_eq!(state.failures, 0);
}
#[test]
fn touch_tracking_slot_release_keeps_position_and_ignores_time_only() {
    let mut slots = TouchSlots::default();
    slots.event(3, 0x2f, 1);
    slots.event(3, 0x39, 7);
    assert_eq!(slots.event(0, 0, 0), Some([false, true]));
    slots.event(1, 0x14a, 1);
    assert_eq!(slots.event(0, 0, 0), Some([false, true]));
    slots.event(3, 0x39, -1);
    assert_eq!(slots.event(0, 0, 0), Some([false, false]));
    let mut mouse = Mouse::default();
    let p = Point { x: 20.0, y: 40.0 };
    assert!(mouse.sample(1, p, true, 1.0).unwrap().pressed);
    assert!(!mouse.sample(1, p, true, 1.1).unwrap().pressed);
    assert!(mouse.sample(1, p, true, 1.2).is_none());
    let release = mouse.sample(1, Point::default(), false, 1.3).unwrap();
    assert!(release.released);
    assert_eq!(release.pos.x, 20.0);
}
fn event(y: f32, pressed: bool, released: bool, time: f64) -> MouseEvent {
    MouseEvent {
        pos: Point { x: 20.0, y },
        slot: 0,
        pressed,
        released,
        down: !released,
        time,
    }
}
#[test]
fn scroll_drag_inertia_and_bounds() {
    let mut scroll = Scroll::default();
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    scroll.update(
        bounds,
        500.0,
        &[
            event(90.0, true, false, 1.0),
            event(60.0, false, false, 1.1),
            event(20.0, false, false, 1.2),
            event(20.0, false, true, 1.3),
        ],
        0.0,
    );
    assert!(scroll.offset < -40.0);
    assert!(scroll.velocity < -300.0);
    for _ in 0..200 {
        scroll.update(bounds, 500.0, &[], 0.0);
    }
    assert!(scroll.offset >= -400.0 && scroll.offset <= 0.0);
}
#[derive(Default)]
struct Trace {
    spawns: u8,
    sends: Vec<Vec<u8>>,
    kills: usize,
    waits: usize,
}
struct FakeLaunch(Rc<RefCell<Trace>>);
struct FakeChild(Rc<RefCell<Trace>>, bool);
impl Launch for FakeLaunch {
    type Child = FakeChild;
    fn spawn(&mut self, _: Window<'_>) -> Result<FakeChild, Error> {
        let mut t = self.0.borrow_mut();
        t.spawns += 1;
        Ok(FakeChild(self.0.clone(), t.spawns == 1))
    }
    fn warning(&mut self, _: &str) {}
}
impl Child for FakeChild {
    fn status(&mut self) -> Result<Option<i32>, Error> {
        Ok(None)
    }
    fn send(&mut self, payload: &[u8]) -> std::io::Result<bool> {
        self.0.borrow_mut().sends.push(payload.into());
        Ok(!self.1)
    }
    fn kill(&mut self) -> std::io::Result<()> {
        self.0.borrow_mut().kills += 1;
        Ok(())
    }
    fn terminate(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn communicate(&mut self, timeout: Duration) -> Result<(), StopError> {
        assert_eq!(timeout, Duration::from_secs(2));
        self.0.borrow_mut().waits += 1;
        Ok(())
    }
}
#[test]
fn broken_pipe_restarts_once_replays_update_and_reaps_on_close() {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut spinner = children::Spinner::new(FakeLaunch(trace.clone()));
    assert!(spinner.update("한글").unwrap());
    assert_eq!(spinner.attempts(), 2);
    spinner.close();
    spinner.close();
    assert!(!spinner.update("after-close").unwrap());
    let t = trace.borrow();
    assert_eq!(t.spawns, 2);
    assert_eq!(t.sends, vec!["한글\n".as_bytes(), "한글\n".as_bytes()]);
    assert_eq!((t.kills, t.waits), (2, 2));
}
struct Canvas;
impl Measure for Canvas {
    fn measure(&self, font: Font, text: &str, size: f32, spacing: f32) -> Point {
        Fixed.measure(font, text, size, spacing)
    }
}
impl openpilot_startup_ui::draw::Draw for Canvas {
    fn text(&mut self, _: openpilot_startup_ui::draw::TextDraw<'_>) -> Result<(), Error> {
        Ok(())
    }
    fn rounded(&mut self, _: Rect, _: f32, _: u32) -> Result<(), Error> {
        Ok(())
    }
    fn border(&mut self, _: Rect, _: f32, _: u32) -> Result<(), Error> {
        Ok(())
    }
    fn texture(&mut self, _: bool, _: Rect, _: Point, _: f32) -> Result<(), Error> {
        Ok(())
    }
    fn scissor(&mut self, _: Option<Rect>) -> Result<(), Error> {
        Ok(())
    }
}
#[test]
fn button_requires_tracked_press_and_supports_reentry() {
    use openpilot_startup_ui::text_window::TextWindow;
    let config = config();
    let rect = TextWindow::button(config);
    let inside = Point {
        x: rect.x + 5.0,
        y: rect.y + 5.0,
    };
    let outside = Point { x: 0.0, y: 0.0 };
    let mut viewer = TextWindow::new("error", config, &Canvas);
    let mut canvas = Canvas;
    let e = |pos, pressed, released| MouseEvent {
        pos,
        slot: 0,
        pressed,
        released,
        down: !released,
        time: 1.0,
    };
    assert!(!viewer
        .render(
            config,
            "no network",
            &[e(inside, false, true)],
            0.0,
            &mut canvas
        )
        .unwrap());
    assert!(!viewer
        .render(
            config,
            "no network",
            &[
                e(inside, true, false),
                e(outside, false, false),
                e(outside, false, true)
            ],
            0.0,
            &mut canvas
        )
        .unwrap());
    assert!(viewer
        .render(
            config,
            "no network",
            &[
                e(inside, true, false),
                e(outside, false, false),
                e(inside, false, false),
                e(inside, false, true)
            ],
            0.0,
            &mut canvas
        )
        .unwrap());
    assert!(viewer.closed);
}
