#[expect(
    clippy::struct_excessive_bools,
    reason = "Preserves the flat source RouteReplayToolAction contract consumed by cluster main"
)]
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteReplayToolAction {
    pub seek_s: Option<f64>,
    pub toggle_pause: bool,
    pub closed: bool,
    pub open_path: Option<String>,
    pub previous_log: bool,
    pub next_log: bool,
    pub option_updates: Vec<(String, bool)>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCommand {
    Seek {
        #[serde(default)]
        value: f64,
    },
    TogglePause,
    Closed,
    OpenPath {
        #[serde(default)]
        value: String,
    },
    PreviousLog,
    NextLog,
    Option {
        #[serde(default)]
        name: String,
        #[serde(default)]
        value: bool,
    },
    #[serde(other)]
    Unknown,
}
#[must_use]
pub fn fold_commands(
    commands: impl IntoIterator<Item = ToolCommand>,
) -> Option<RouteReplayToolAction> {
    let mut action = RouteReplayToolAction::default();
    let mut received = false;
    for command in commands {
        received = true;
        match command {
            ToolCommand::Seek { value } => action.seek_s = Some(value),
            ToolCommand::TogglePause => action.toggle_pause = !action.toggle_pause,
            ToolCommand::Closed => action.closed = true,
            ToolCommand::OpenPath { value } => {
                let value = value.trim_matches(openpilot_startup_ui::text::whitespace);
                action.open_path = if value.is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                };
            }
            ToolCommand::PreviousLog => action.previous_log = true,
            ToolCommand::NextLog => action.next_log = true,
            ToolCommand::Option { name, value } => {
                if !name.is_empty() {
                    if let Some((_, current)) = action
                        .option_updates
                        .iter_mut()
                        .find(|(key, _)| *key == name)
                    {
                        *current = value;
                    } else {
                        action.option_updates.push((name, value));
                    }
                }
            }
            ToolCommand::Unknown => {}
        }
    }
    received.then_some(action)
}
