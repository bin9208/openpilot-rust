use openpilot_startup_ui::{config::Config, renderer::Renderer};
use openpilot_ui_framework::{
    button::{Button, ButtonStyle},
    canvas::Canvas,
    geometry::{MouseEvent, Rect},
    label::Label,
    text::Font,
    text_layout::{Horizontal, Vertical},
    toggle::Toggle,
    unified_label::UnifiedLabel,
    widget::{Frame, Widget},
};
use serde::Deserialize;
use std::path::Path;
#[derive(Deserialize)]
struct Scene {
    config: Config,
    #[serde(default)]
    language: String,
    frames: usize,
    elements: Vec<Element>,
}
#[derive(Deserialize)]
struct Element {
    kind: String,
    rect: Rect,
    #[serde(default)]
    text: String,
    #[serde(default)]
    props: serde_json::Value,
    #[serde(default)]
    events: Vec<MouseEvent>,
}
fn number(value: &serde_json::Value, key: &str, default: f64) -> f64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(default)
}
fn boolean(value: &serde_json::Value, key: &str, default: bool) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(default)
}
fn word<'a>(value: &'a serde_json::Value, key: &str, default: &'a str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default)
}
fn font(value: &str) -> Font {
    match value {
        "medium" => Font::Medium,
        "bold" => Font::Bold,
        "semi_bold" => Font::SemiBold,
        "display" => Font::Display,
        "regular" => Font::Regular,
        "pretendard" => Font::Pretendard,
        "unifont" => Font::Unifont,
        _ => Font::Normal,
    }
}
fn alignment(value: &str) -> Horizontal {
    match value {
        "center" => Horizontal::Center,
        "right" => Horizontal::Right,
        _ => Horizontal::Left,
    }
}
fn vertical(value: &str) -> Vertical {
    match value {
        "middle" => Vertical::Middle,
        "bottom" => Vertical::Bottom,
        _ => Vertical::Top,
    }
}
enum ElementWidget {
    Label(Label),
    Unified(UnifiedLabel),
    Button(Button),
    Toggle(Toggle),
}
impl ElementWidget {
    fn widget(&mut self) -> &mut dyn Widget {
        match self {
            Self::Label(value) => value,
            Self::Unified(value) => value,
            Self::Button(value) => value,
            Self::Toggle(value) => value,
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output] = args.as_slice() else {
        return Err("usage: ui_render ROOT SCENE PNG".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let renderer = Renderer::new(scene.config, &assets, false, &scene.language)?;
    let mut canvas = Canvas::new(renderer, &assets);
    let mut widgets = Vec::new();
    for element in &scene.elements {
        let props = &element.props;
        let mut widget = match element.kind.as_str() {
            "label" => {
                let mut label = Label::new(&element.text);
                label.size = number(props, "size", 60.0);
                label.font = font(word(props, "font", "normal"));
                label.padding = number(props, "padding", 0.0);
                label.elide = boolean(props, "elide", false);
                label.horizontal = alignment(word(props, "horizontal", "center"));
                label.vertical = vertical(word(props, "vertical", "middle"));
                ElementWidget::Label(label)
            }
            "unified" => {
                let mut label = UnifiedLabel::new(&element.text);
                label.size = number(props, "size", 60.0);
                label.font = font(word(props, "font", "normal"));
                label.padding = number(props, "padding", 0.0);
                label.elide = boolean(props, "elide", true);
                label.wrap = boolean(props, "wrap", true);
                label.scroll = boolean(props, "scroll", false);
                label.shimmer = boolean(props, "shimmer", false);
                label.letter_spacing = number(props, "letter_spacing", 0.0);
                label.line_height = number(props, "line_height", 1.0);
                label.horizontal = alignment(word(props, "horizontal", "left"));
                label.vertical = vertical(word(props, "vertical", "top"));
                ElementWidget::Unified(label)
            }
            "button" => {
                let mut button = Button::new(&element.text);
                button.label.size = number(props, "size", 60.0);
                button.set_style(match word(props, "style", "normal") {
                    "primary" => ButtonStyle::Primary,
                    "danger" => ButtonStyle::Danger,
                    "list" => ButtonStyle::ListAction,
                    "border" => ButtonStyle::TransparentWhiteBorder,
                    _ => ButtonStyle::Normal,
                });
                ElementWidget::Button(button)
            }
            "toggle" => ElementWidget::Toggle(Toggle::new(boolean(props, "value", false))),
            _ => return Err(format!("unknown widget {}", element.kind).into()),
        };
        widget.widget().set_rect(element.rect);
        widget.widget().state_mut().enabled = boolean(props, "enabled", true).into();
        widgets.push(widget);
    }
    for index in 0..scene.frames {
        canvas.renderer.begin();
        for (widget, element) in widgets.iter_mut().zip(&scene.elements) {
            use num_traits::ToPrimitive;
            let events = if index == 0 {
                element.events.as_slice()
            } else {
                &[]
            };
            let frame = Frame {
                now: index.to_f64().ok_or("frame overflow")? / 20.0,
                monotonic: 0.0,
                keyboard: &Default::default(),
                navigation: &Default::default(),
                dt: 0.05,
                target_fps: 20.0,
                awake: true,
                events,
                last_event: events.last().copied().unwrap_or_default(),
                cursor: openpilot_ui_framework::geometry::Point::default(),
                wheel: 0.0,
                show_touches: false,
            };
            widget.widget().render(&frame, &mut canvas)?;
        }
        if index + 1 == scene.frames {
            canvas.renderer.screenshot(Path::new(output))?;
        }
        canvas.renderer.end();
    }
    let mut result = Vec::new();
    for widget in widgets {
        result.push(match widget {
            ElementWidget::Unified(label) => {
                serde_json::json!({"width":label.text_width(),"scroll_offset":label.scroll_offset})
            }
            ElementWidget::Toggle(toggle) => {
                serde_json::json!({"value":toggle.value(),"progress":toggle.progress()})
            }
            _ => serde_json::Value::Null,
        });
    }
    std::fs::write(
        Path::new(output).with_extension("json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(())
}
