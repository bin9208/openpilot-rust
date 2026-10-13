use super::{DebugState, Settings};
use crate::{
    live::json,
    models::{NaviDebugInfo, NaviGuidanceImage, NaviTrafficLightInfo},
};
use num_traits::ToPrimitive;
use openpilot_logmessaged::{JsonValue, JsonView};

impl DebugState {
    pub(super) fn refresh_navigation(&mut self, params: &Settings, now: f64) {
        let params = params.memory.as_ref().unwrap_or(&params.persistent);
        self.navi_image = json::read(params, "CarrotNaviImage").and_then(|data| image(&data));
        self.navi_debug = json::read(params, "CarrotNaviDebug").map(|data| {
            let received = data
                .get("receivedMono")
                .and_then(|value| json::finite(&value))
                .unwrap_or(now);
            if let Some(light) = data.get("trafficLight").as_ref().and_then(traffic_light) {
                if now - received <= 10.0 {
                    self.held_light = Some((light, received));
                }
            }
            let lines = match data.get("lines").and_then(|value| {
                if let JsonView::Array(lines) = value.view() {
                    Some(lines)
                } else {
                    None
                }
            }) {
                Some(lines) => lines
                    .into_iter()
                    .filter_map(|value| json::text(&value))
                    .filter(|value| !openpilot_startup_ui::text::trim(value).is_empty())
                    .take(12)
                    .map(|value| json::truncate(&value, 80))
                    .collect(),
                _ => Vec::new(),
            };
            NaviDebugInfo {
                title: json::truncate(&json::field_text(&data, "title", "Navi"), 80),
                severity: json::truncate(&json::field_text(&data, "severity", "normal"), 16),
                lines,
                speed_limit_kph: json::positive(data.get("speedLimitKph"), 300),
                traffic_light: None,
                guidance_image: None,
            }
        });
    }
    pub(in crate::live) fn navigation_info(
        &mut self,
        now: f64,
        route_points: usize,
    ) -> NaviDebugInfo {
        let mut info = self.navi_debug.clone().unwrap_or_else(|| NaviDebugInfo {
            title: "NAVI receiver".into(),
            lines: vec![
                format!("Route points: {route_points}"),
                "Waiting for POST /api/navi/{appVersion}".into(),
            ],
            ..NaviDebugInfo::default()
        });
        info.guidance_image.clone_from(&self.navi_image);
        info.traffic_light = self.held_at(now);
        info
    }
    fn held_at(&mut self, now: f64) -> Option<NaviTrafficLightInfo> {
        let (light, received) = self.held_light.as_ref()?;
        let age = (now - received).max(0.0);
        if age > 10.0 {
            self.held_light = None;
            return None;
        }
        let elapsed = age.trunc().to_i64()?;
        let mut light = light.clone();
        for seconds in [
            &mut light.red_s,
            &mut light.straight_s,
            &mut light.left_s,
            &mut light.right_s,
            &mut light.uturn_s,
        ] {
            *seconds = seconds.map(|value| (value - elapsed).max(0));
        }
        Some(light)
    }
}
fn image(data: &JsonValue) -> Option<NaviGuidanceImage> {
    let mut image_base64 = json::field_text(data, "imageBase64", "");
    let image_hash = json::truncate(&json::field_text(data, "imageHash", ""), 32);
    let width = json::positive(data.get("imageWidth"), 10000).unwrap_or(0);
    let height = json::positive(data.get("imageHeight"), 10000).unwrap_or(0);
    if !image_base64.is_empty()
        && (image_base64.chars().count() > 2 * 1024 * 1024 || width > 2048 || height > 2048)
    {
        image_base64.clear();
    }
    if image_base64.is_empty() && image_hash.is_empty() {
        return None;
    }
    Some(NaviGuidanceImage {
        image_base64,
        image_hash,
        width,
        height,
        image_mime: json::truncate(&json::field_text(data, "imageMime", ""), 64),
    })
}
fn traffic_light(data: &JsonValue) -> Option<NaviTrafficLightInfo> {
    data.is_object().then(|| NaviTrafficLightInfo {
        distance_m: json::positive(data.get("distanceM"), 10000),
        red_s: json::positive(data.get("redS"), 999),
        straight_s: json::positive(data.get("straightS"), 999),
        left_s: json::positive(data.get("leftS"), 999),
        right_s: json::positive(data.get("rightS"), 999),
        uturn_s: json::positive(data.get("uturnS"), 999),
        red_on: json::optional_bool(data.get("redOn")),
        straight_on: json::optional_bool(data.get("straightOn")),
        left_on: json::optional_bool(data.get("leftOn")),
        right_on: json::optional_bool(data.get("rightOn")),
        uturn_on: json::optional_bool(data.get("uturnOn")),
        meta: None,
    })
}
