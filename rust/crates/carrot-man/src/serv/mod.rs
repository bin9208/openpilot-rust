mod detection;
mod gas;
mod gps;
mod projection;
pub mod settings;
mod speed;
mod tick;
mod turn;
pub mod types;
pub use projection::TrafficAction;
pub use tick::Decision;
pub use turn::turn_mapping;
mod description;
use crate::navigation::Projected;
use serde::Serialize;
use std::collections::VecDeque;
pub use types::*;

#[derive(Clone, Debug, Serialize)]
pub struct CarrotServ {
    pub nav: NavigationState,
    pub speed: SpeedState,
    pub gps: GpsState,
    pub command: CommandState,
    pub settings: settings::Settings,
    pub projected: Option<Projected>,
    pub sequences: [u64; 6],
    pub session_id: String,
    pub off_route: bool,
    pub navi_active: bool,
    pub has_control: bool,
    pub road_limit_valid: bool,
    pub traffic_active: bool,
    pub disabled_safety: Option<(
        crate::sources::Source,
        String,
        Option<crate::sources::SafetyItem>,
        Option<crate::sources::SafetyItem>,
    )>,
    pub detection: VecDeque<Detection>,
    pub traffic_light_count: i64,
    pub traffic_state: i64,
    #[serde(skip)]
    pub main_text_python_json: Option<String>,
}

impl CarrotServ {
    pub fn new(settings: settings::Settings) -> Self {
        Self {
            nav: NavigationState::default(),
            speed: SpeedState::default(),
            gps: GpsState::default(),
            command: CommandState::default(),
            settings,
            projected: None,
            sequences: [0; 6],
            session_id: String::new(),
            off_route: false,
            navi_active: false,
            has_control: false,
            road_limit_valid: false,
            traffic_active: false,
            disabled_safety: None,
            detection: VecDeque::new(),
            traffic_light_count: -1,
            traffic_state: 0,
            main_text_python_json: None,
        }
    }

    pub fn safety_identity(
        &self,
    ) -> Option<(
        crate::sources::Source,
        String,
        Option<crate::sources::SafetyItem>,
        Option<crate::sources::SafetyItem>,
    )> {
        self.projected
            .as_ref()?
            .selection
            .snapshot
            .as_ref()
            .map(|s| {
                (
                    s.source,
                    s.session_id.clone(),
                    s.control.safety.clone(),
                    s.control.secondary_safety.clone(),
                )
            })
    }
}
