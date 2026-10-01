//! Animated compact keyboard from system/ui/widgets/mici_keyboard.py.
mod input;
mod key;
mod render;
use crate::{
    animation::{Bounce, Filter},
    assets::Texture,
    widget::WidgetState,
    Error,
};
pub use key::{Key, Kind};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum CapsState {
    #[default]
    Lower,
    Upper,
    Lock,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Layer {
    #[default]
    Lower,
    Upper,
    Special,
    SuperSpecial,
}
impl Layer {
    fn index(self) -> usize {
        match self {
            Self::Lower => 0,
            Self::Upper => 1,
            Self::Special => 2,
            Self::SuperSpecial => 3,
        }
    }
}
pub struct MiciKeyboard {
    pub state: WidgetState,
    pub layer: Layer,
    pub caps: CapsState,
    pub keys: Vec<Key>,
    pub closest: Option<(usize, f64)>,
    pub selected_at: Option<f64>,
    pub unselect_at: Option<f64>,
    pub dragging: bool,
    pub text: String,
    pub background_scale: Bounce,
    pub selected_filter: Filter,
    pub auto_return: String,
    rows: [Vec<Vec<usize>>; 4],
    caps_key: usize,
    number_keys: [usize; 2],
    abc_key: usize,
    special_key: usize,
    touch_started: bool,
    release_started: bool,
    initialized: bool,
    background: Texture,
    caps_icons: [Texture; 3],
}
impl MiciKeyboard {
    pub fn new(
        fps: f64,
        mut texture: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let mut keys = Vec::new();
        let mut rows: [Vec<Vec<usize>>; 4] = Default::default();
        for (index, letters) in [
            ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
            ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
            ["1234567890", "-/:;()$&@\"", "~.,?!'#%"],
            ["1234567890", "`[]{}^*+=_", "\\|<>¥€£•"],
        ]
        .into_iter()
        .enumerate()
        {
            for row in letters {
                let mut ids = Vec::new();
                for letter in row.chars() {
                    ids.push(keys.len());
                    keys.push(Key::new(&letter.to_string(), Kind::Character, fps));
                }
                rows[index].push(ids);
            }
        }
        let mut space = Key::new(" ", Kind::Icon { bottom: true }, fps);
        space.icon = Some(texture("icons_mici/settings/keyboard/space.png", (43, 14))?);
        let space_key = keys.len();
        keys.push(space);
        let caps_icons = [
            texture("icons_mici/settings/keyboard/caps_lower.png", (38, 33))?,
            texture("icons_mici/settings/keyboard/caps_upper.png", (38, 33))?,
            texture("icons_mici/settings/keyboard/caps_lock.png", (39, 38))?,
        ];
        let mut caps = Key::new("", Kind::Icon { bottom: false }, fps);
        caps.icon = Some(caps_icons[0]);
        let caps_key = keys.len();
        keys.push(caps);
        let first_number = keys.len();
        keys.push(Key::new("123", Kind::Small, fps));
        let second_number = keys.len();
        keys.push(Key::new("123", Kind::Small, fps));
        let abc_key = keys.len();
        keys.push(Key::new("abc", Kind::Small, fps));
        let special_key = keys.len();
        keys.push(Key::new("#+=", Kind::Small, fps));
        for row in &mut rows[..2] {
            row[2].insert(0, caps_key);
            row[2].push(first_number);
        }
        for row in &mut rows {
            row[1].push(space_key);
        }
        for row in &mut rows[2..] {
            row[2].push(abc_key);
        }
        rows[2][2].insert(0, special_key);
        rows[3][2].insert(0, second_number);
        let background = texture(
            "icons_mici/settings/keyboard/keyboard_background.png",
            (520, 170),
        )?;
        Ok(Self {
            state: WidgetState::default(),
            layer: Layer::Lower,
            caps: CapsState::Lower,
            keys,
            closest: None,
            selected_at: None,
            unselect_at: None,
            dragging: false,
            text: String::new(),
            background_scale: Bounce::new(1.0, 0.065, fps, 2.0),
            selected_filter: Filter::new(0.0, 0.04875, fps),
            auto_return: String::new(),
            rows,
            caps_key,
            number_keys: [first_number, second_number],
            abc_key,
            special_key,
            touch_started: false,
            release_started: false,
            initialized: false,
            background,
            caps_icons,
        })
    }
    pub fn active_keys(&self) -> impl Iterator<Item = &Key> {
        self.rows[self.layer.index()]
            .iter()
            .flatten()
            .map(|id| &self.keys[*id])
    }
    pub fn candidate(&self) -> &str {
        self.closest
            .filter(|(id, _)| self.dragging && self.keys[*id].kind == Kind::Character)
            .map_or("", |(id, _)| self.keys[id].value.as_str())
    }
    pub fn height(&self) -> f32 {
        self.background.height
    }
    pub fn backspace(&mut self) {
        self.text.pop();
    }
    pub fn space(&mut self) {
        self.text.push(' ');
    }
    fn set_layer(&mut self, layer: Layer) {
        for (old, new) in self.rows[self.layer.index()]
            .iter()
            .zip(&self.rows[layer.index()])
        {
            for index in 0..old.len().max(new.len()) {
                let previous = old[index.min(old.len() - 1)];
                let next = new[index.min(new.len() - 1)];
                let position = (
                    f64::from(self.keys[previous].rect.x),
                    f64::from(self.keys[previous].rect.y),
                );
                self.keys[next].position(position, f64::from(self.state.rect.y), false);
            }
        }
        self.layer = layer;
    }
    pub fn uppercase(&mut self, cycle: bool) {
        self.set_layer(if cycle { Layer::Upper } else { Layer::Lower });
        if !cycle {
            self.caps = CapsState::Lower;
            self.keys[self.caps_key].icon = Some(self.caps_icons[0]);
        } else {
            match self.caps {
                CapsState::Lower => {
                    self.caps = CapsState::Upper;
                    self.keys[self.caps_key].icon = Some(self.caps_icons[1]);
                }
                CapsState::Upper => {
                    self.caps = CapsState::Lock;
                    self.keys[self.caps_key].icon = Some(self.caps_icons[2]);
                }
                CapsState::Lock => self.uppercase(false),
            }
        }
    }
}
