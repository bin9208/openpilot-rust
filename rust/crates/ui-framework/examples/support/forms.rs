use super::*;
#[derive(Deserialize)]
pub(super) struct Scene {
    pub(super) config: Config,
    #[serde(default)]
    pub(super) language: String,
    pub(super) kind: String,
    pub(super) rect: Rect,
    #[serde(default)]
    pub(super) text: String,
    pub(super) frames: usize,
    #[serde(default)]
    pub(super) props: serde_json::Value,
    #[serde(default)]
    pub(super) actions: Vec<Action>,
}
#[derive(Deserialize)]
pub(super) struct Action {
    pub(super) frame: usize,
    #[serde(default)]
    pub(super) operation: String,
    #[serde(default)]
    pub(super) text: String,
    #[serde(default)]
    pub(super) value: usize,
    #[serde(default)]
    pub(super) key: i32,
    #[serde(default)]
    pub(super) down: Vec<i32>,
    #[serde(default)]
    pub(super) events: Vec<MouseEvent>,
}
pub(super) fn number(p: &serde_json::Value, k: &str, d: f64) -> f64 {
    p.get(k).and_then(serde_json::Value::as_f64).unwrap_or(d)
}
pub(super) fn flag(p: &serde_json::Value, k: &str) -> bool {
    p.get(k)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}
pub(super) fn string<'a>(p: &'a serde_json::Value, k: &str, d: &'a str) -> &'a str {
    p.get(k).and_then(serde_json::Value::as_str).unwrap_or(d)
}
pub(super) enum Form {
    Mici(Box<MiciKeyboard>),
    Input(Box<InputBox>),
    Html(Box<HtmlRenderer>),
    Keyboard(Box<Keyboard>),
    Confirm(Box<ConfirmDialog>),
    Options(Box<MultiOptionDialog>),
    List(Box<ListItem>),
    Slider(Box<Slider>),
}
impl Form {
    pub(super) fn widget(&mut self) -> &mut dyn Widget {
        match self {
            Self::Mici(v) => v.as_mut(),
            Self::Input(v) => v.as_mut(),
            Self::Html(v) => v.as_mut(),
            Self::Keyboard(v) => v.as_mut(),
            Self::Confirm(v) => v.as_mut(),
            Self::Options(v) => v.as_mut(),
            Self::List(v) => v.as_mut(),
            Self::Slider(v) => v.as_mut(),
        }
    }
    pub(super) fn snapshot(&mut self, canvas: &Canvas, now: f64) -> serde_json::Value {
        match self {
            Self::Mici(v) => {
                serde_json::json!({"text":v.text,"candidate":v.candidate(),"caps":v.caps,"layer":v.layer,"selected_at":v.selected_at,"unselect_at":v.unselect_at,"dragging":v.dragging,"keys":v.active_keys().map(|key|serde_json::json!({"value":key.value,"rect":key.rect,"original":key.original,"size":key.size.position.x,"alpha":key.alpha.position.x})).collect::<Vec<_>>()})
            }
            Self::Input(v) => {
                serde_json::json!({"text":v.text(),"cursor":v.cursor(),"offset":v.offset(),"show_cursor":v.show_cursor(),"display":v.display_text(now)})
            }
            Self::Html(v) => {
                serde_json::json!({"height":v.total_height(canvas,f64::from(v.state.rect.width).trunc()),"elements":v.elements})
            }
            Self::Keyboard(v) => {
                serde_json::json!({"text":v.text(),"layout":v.layout,"caps":v.caps_lock,"cursor":v.input.cursor(),"display":v.input.display_text(now)})
            }
            Self::Options(v) => serde_json::json!({"selection":*v.selection.borrow()}),
            Self::List(v) => {
                serde_json::json!({"description_visible":v.description_visible,"height":v.state.rect.height,"right":v.right_rect(canvas)})
            }
            Self::Slider(v) => {
                serde_json::json!({"confirmed":v.confirmed(),"percentage":v.percentage(),"position":v.position.x,"scale":v.scale.position.x,"dragging":v.dragging})
            }
            Self::Confirm(_) => serde_json::Value::Null,
        }
    }
}
