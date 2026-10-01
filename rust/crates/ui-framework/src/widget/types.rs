pub enum Property<T> {
    Value(T),
    Dynamic(Box<dyn Fn() -> T>),
}
impl<T: Clone> Property<T> {
    pub fn get(&self) -> T {
        match self {
            Self::Value(value) => value.clone(),
            Self::Dynamic(get) => get(),
        }
    }
}
impl<T> From<T> for Property<T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    Cancel,
    Confirm,
    NoAction,
}
impl DialogResult {
    pub const fn code(self) -> i32 {
        match self {
            Self::Cancel => 0,
            Self::Confirm => 1,
            Self::NoAction => -1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RenderResult {
    #[default]
    None,
    Bool(bool),
    Dialog(DialogResult),
    Value(i32),
    Float(f64),
}

impl RenderResult {
    pub fn truthy(self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => value,
            Self::Dialog(value) => value.code() != 0,
            Self::Value(value) => value != 0,
            Self::Float(value) => value != 0.0,
        }
    }
}
