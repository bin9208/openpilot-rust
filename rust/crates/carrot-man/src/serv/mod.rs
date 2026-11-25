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
    /// Monotonic time until which the turn floor applies (see curve_floor).
    #[serde(skip)]
    pub turn_floor_until: f64,
}

/// How long the turn floor outlives ATC's last "turn left/right": the
/// instruction moves on to the next maneuver a few metres past the turn
/// point (route 00000ce7, 3 m), while the car is still in the turn.
pub const TURN_FLOOR_HOLD_S: f64 = 8.;

impl CarrotServ {
    pub fn new(settings: settings::Settings) -> Self {
        Self {
            nav: NavigationState::default(),
            speed: SpeedState::default(),
            gps: GpsState {
                position_source: "none",
                ..GpsState::default()
            },
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
            turn_floor_until: 0.,
        }
    }

    /// The floor for the vision-curve and route-turn speeds. Intersection
    /// turns need less than AutoCurveSpeedLowerLimit: at 22 km/h a 10 m turn
    /// asks 3-4 m/s2 and saturates the steering (routes 00000ce7/ce8), so
    /// while ATC reports a turn (and TURN_FLOOR_HOLD_S after) the lower
    /// AutoCurveSpeedTurnLowerLimit applies when it is set.
    pub fn curve_floor(&self, now: f64) -> f64 {
        let turn = self.settings.turn_lower_limit;
        if turn > 0. && now < self.turn_floor_until {
            self.settings.curve_lower_limit.min(turn)
        } else {
            self.settings.curve_lower_limit
        }
    }

    /// Arms the turn floor while ATC's current maneuver is an intersection
    /// turn inside its turn-start distance.
    pub fn note_turn(&mut self, kind: &str, now: f64) {
        if matches!(kind, "turn left" | "turn right") && now.is_finite() {
            self.turn_floor_until = now + TURN_FLOOR_HOLD_S;
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

pub mod gps_sources;

#[cfg(test)]
mod turn_floor_tests {
    use super::{settings::Settings, CarrotServ, TURN_FLOOR_HOLD_S};

    fn serv(curve: f64, turn: f64) -> CarrotServ {
        let mut settings: Settings = serde_json::from_value(serde_json::json!({"bump_speed":0.,"bump_time":0.,"bump_end_distance":0.,"control_end":0.,"rear_hold_distance":0.,"control_mode":0,"vehicle_can_control":0,"school_control":false,"camera_control_mode":0,"safety_factor":0.,"deceleration":0.,"countdown_mode":0,"turn_speed_mode":0,"map_turn_factor":0.,"auto_turn_speed":0.,"auto_turn_map_change":0,"auto_turn_control":0,"auto_turn_end":0.,"curve_lower_limit":0.,"is_metric":true,"road_limit_offset":0.,"curve_factor":0.,"language":"en"})).unwrap();
        assert_eq!(settings.turn_lower_limit, 0.);
        settings.curve_lower_limit = curve;
        settings.turn_lower_limit = turn;
        CarrotServ::new(settings)
    }

    #[test]
    fn the_turn_floor_applies_only_around_an_intersection_turn() {
        let mut s = serv(22., 12.);
        assert_eq!(s.curve_floor(100.), 22.);
        for kind in [
            "turn left prepare",
            "atc left",
            "fork right",
            "straight",
            "none",
        ] {
            s.note_turn(kind, 100.);
            assert_eq!(s.curve_floor(100.), 22., "{kind}");
        }
        s.note_turn("turn right", 100.);
        assert_eq!(s.curve_floor(100.), 12.);
        // Held past the turn point, then back to the ordinary floor.
        assert_eq!(s.curve_floor(100. + TURN_FLOOR_HOLD_S - 0.1), 12.);
        assert_eq!(s.curve_floor(100. + TURN_FLOOR_HOLD_S), 22.);
    }

    #[test]
    fn off_or_higher_than_the_curve_floor_changes_nothing() {
        let mut off = serv(22., 0.);
        off.note_turn("turn left", 1.);
        assert_eq!(off.curve_floor(1.), 22.);
        let mut higher = serv(22., 30.);
        higher.note_turn("turn left", 1.);
        assert_eq!(higher.curve_floor(1.), 22.);
    }
}
