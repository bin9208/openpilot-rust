use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum Source {
    #[serde(rename = "tmap_legacy")]
    TmapLegacy,
    #[serde(rename = "carrot_navi_v2")]
    CarrotNaviV2,
    #[serde(rename = "naver_v1")]
    NaverV1,
}
impl Source {
    pub const fn lease(self) -> f64 {
        match self {
            Self::TmapLegacy => 4.,
            Self::CarrotNaviV2 => 10.,
            Self::NaverV1 => 2.,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::TmapLegacy => "tmap_legacy",
            Self::CarrotNaviV2 => "carrot_navi_v2",
            Self::NaverV1 => "naver_v1",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    Idle,
    Guiding,
    Stopped,
    Arrived,
}
impl Lifecycle {
    pub const fn terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Arrived)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Instruction {
    pub present: bool,
    pub turn_type: i64,
    pub distance_m: f64,
    pub road_name: String,
    pub main_text: String,
    #[serde(skip)]
    pub original_main_text_json: Option<String>,
    pub near_direction: String,
    pub far_direction: String,
    pub next_road_width: i64,
    pub received_mono_s: Option<f64>,
}
impl Default for Instruction {
    fn default() -> Self {
        Self {
            present: false,
            turn_type: -1,
            distance_m: 0.,
            road_name: String::new(),
            main_text: String::new(),
            original_main_text_json: None,
            near_direction: String::new(),
            far_direction: String::new(),
            next_road_width: 0,
            received_mono_s: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct SafetyItem {
    #[serde(rename = "type")]
    pub kind: i64,
    pub distance_m: f64,
    pub speed_limit_kph: f64,
    pub received_mono_s: f64,
    #[serde(default = "camera")]
    pub reason: String,
    #[serde(default)]
    pub section: bool,
    #[serde(default = "negative")]
    pub section_type: i64,
    #[serde(default = "negative")]
    pub block_type: i64,
    #[serde(default)]
    pub block_speed_kph: f64,
    #[serde(default)]
    pub block_distance_m: f64,
    #[serde(default)]
    pub revision: Option<u64>,
}
fn camera() -> String {
    "cam".into()
}
fn negative() -> i64 {
    -1
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Control {
    pub current: Instruction,
    pub next: Instruction,
    pub safety: Option<SafetyItem>,
    pub secondary_safety: Option<SafetyItem>,
    pub speed_present: bool,
    pub speed_received_mono_s: Option<f64>,
    pub road_limit_kph: Option<f64>,
    pub road_limit_received_mono_s: Option<f64>,
    pub road_category: Option<i64>,
    pub road_category_received_mono_s: Option<f64>,
    pub route_present: bool,
    pub route_revision: Option<u64>,
    pub route_received_mono_s: Option<f64>,
    pub remaining_distance_m: f64,
    pub remaining_time_s: f64,
    pub off_route: bool,
    pub status_present: bool,
    pub status_received_mono_s: Option<f64>,
    pub destination: Option<(f64, f64)>,
    pub destination_present: bool,
    pub destination_received_mono_s: Option<f64>,
    pub route_points: Vec<(f64, f64)>,
    pub position_present: bool,
    pub position_received_mono_s: Option<f64>,
    pub position_latitude: f64,
    pub position_longitude: f64,
    pub position_heading_deg: f64,
    pub position_speed_kph: f64,
    pub position_road_name: String,
    pub traffic_present: bool,
    pub traffic_received_mono_s: Option<f64>,
    pub traffic_visible: bool,
    pub traffic_distance_m: f64,
    pub traffic_source: String,
    pub traffic_lamp: String,
    pub traffic_remain_s: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snapshot {
    pub source: Source,
    pub session_id: String,
    pub sequence: u64,
    pub lifecycle: Lifecycle,
    pub received_mono_s: f64,
    #[serde(default)]
    pub activation_epoch: u64,
    pub control: Control,
    #[serde(default)]
    pub owner_received_mono_s: Option<f64>,
    #[serde(skip)]
    pub original_json: Option<String>,
}
impl Snapshot {
    pub fn owner_receipt(&self) -> f64 {
        self.owner_received_mono_s.unwrap_or(self.received_mono_s)
    }
    pub fn fresh(&self, now: f64) -> bool {
        self.lifecycle == Lifecycle::Guiding
            && super::age(now, self.owner_receipt()) < self.source.lease()
    }
    pub(super) fn normalize(&mut self, now: f64) -> bool {
        let original = self.received_mono_s;
        let c = &mut self.control;
        for item in [&mut c.safety, &mut c.secondary_safety]
            .into_iter()
            .flatten()
        {
            if item.received_mono_s == original {
                item.received_mono_s = now;
            }
            if !item.received_mono_s.is_finite() || item.received_mono_s > now {
                return false;
            }
        }
        if c.road_category.is_none() && c.road_category_received_mono_s.is_some() {
            return false;
        }
        if c.route_present && !c.status_present {
            c.status_present = true;
            c.status_received_mono_s = c.route_received_mono_s;
        }
        if c.destination.is_some() && !c.destination_present {
            c.destination_present = true;
            if c.destination_received_mono_s.is_none() {
                c.destination_received_mono_s = Some(now);
            }
        }
        if !c.speed_present {
            let receipts = [
                c.safety.as_ref().map(|s| s.received_mono_s),
                c.secondary_safety.as_ref().map(|s| s.received_mono_s),
                c.road_limit_received_mono_s,
            ];
            if let Some(receipt) = receipts.into_iter().flatten().max_by(f64::total_cmp) {
                c.speed_present = true;
                c.speed_received_mono_s = Some(receipt);
            }
        }
        for instruction in [&mut c.current, &mut c.next] {
            if instruction.present
                && !normalize_time(&mut instruction.received_mono_s, original, now)
            {
                return false;
            }
        }
        for (present, receipt) in [
            (c.speed_present, &mut c.speed_received_mono_s),
            (
                c.road_limit_kph.is_some(),
                &mut c.road_limit_received_mono_s,
            ),
            (
                c.road_category.is_some(),
                &mut c.road_category_received_mono_s,
            ),
            (c.route_present, &mut c.route_received_mono_s),
            (c.status_present, &mut c.status_received_mono_s),
            (c.destination_present, &mut c.destination_received_mono_s),
            (c.position_present, &mut c.position_received_mono_s),
            (c.traffic_present, &mut c.traffic_received_mono_s),
        ] {
            if present {
                if !normalize_time(receipt, original, now) {
                    return false;
                }
            } else if receipt.is_some() {
                return false;
            }
        }
        true
    }
}
fn normalize_time(receipt: &mut Option<f64>, original: f64, now: f64) -> bool {
    if receipt.is_none() || *receipt == Some(original) {
        *receipt = Some(now);
    }
    receipt.is_some_and(|value| value.is_finite() && value <= now)
}

#[derive(Clone, Debug, Serialize)]
pub struct Selection {
    pub snapshot: Option<Snapshot>,
    pub reason: &'static str,
    pub owner_age_s: Option<f64>,
    pub safety_age_s: Option<f64>,
    pub road_category_age_s: Option<f64>,
    pub projection_revision: u64,
    pub transport_loss_age_s: Option<f64>,
    pub secondary_safety_age_s: Option<f64>,
}
