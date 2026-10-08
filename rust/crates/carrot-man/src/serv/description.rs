use super::{CarrotServ, Decision};
use std::{collections::BTreeMap, sync::LazyLock};

static LABELS: LazyLock<BTreeMap<String, BTreeMap<String, String>>> =
    LazyLock::new(|| serde_json::from_str(include_str!("sdi_labels.json")).unwrap_or_default());

impl CarrotServ {
    pub fn sdi_description(&self, decision: &Decision) -> String {
        if decision.rear_holding && decision.source == "cam" {
            let label = match self.settings.language.as_str() {
                "ko" => "후면단속 속도 유지",
                "zh" => "后向测速限速保持",
                _ => "Rear camera speed hold",
            };
            return format!("{label} {:.0}m", decision.rear_remaining.ceil());
        }
        let kind = if self.nav.sdi_type == 0 && self.nav.sdi_distance == 0. {
            -1
        } else {
            self.nav.sdi_type
        };
        let language = match self.settings.language.as_str() {
            "ko" => "ko",
            "zh" => "zh",
            _ => "en",
        };
        LABELS
            .get(language)
            .and_then(|labels| labels.get(&kind.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}
