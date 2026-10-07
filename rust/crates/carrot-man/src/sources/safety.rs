use super::{SafetyItem, Selection, Source};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct SafetyPolicy {
    pub mode: i64,
    pub safety_factor: f64,
    pub bump_speed_kph: f64,
}

#[derive(Debug, Serialize)]
pub struct SafetyDecision {
    pub provider: &'static str,
    pub reason: &'static str,
    pub limit_kph: f64,
    pub distance_m: f64,
    #[serde(rename = "type")]
    pub kind: i64,
    pub rejection: Option<&'static str>,
}

pub fn navigation_safety(
    selection: &Selection,
    policy: SafetyPolicy,
) -> (Option<SafetyDecision>, Option<&'static str>) {
    let Some(snapshot) = &selection.snapshot else {
        return (None, Some("no_owner"));
    };
    let control = &snapshot.control;
    if control.off_route {
        return (None, Some("off_route"));
    }
    let decide = |item: Option<&SafetyItem>| -> (Option<SafetyDecision>, Option<&'static str>) {
        let Some(item) = item else {
            return (None, None);
        };
        if !matches!(item.kind, 0 | 1 | 2 | 3 | 4 | 7 | 8 | 22 | 75 | 76) {
            return (None, Some("unsupported_type"));
        }
        if !item.distance_m.is_finite() || item.distance_m <= 0. {
            return (None, Some("invalid_distance"));
        }
        if policy.mode <= 0 {
            return (None, Some("mode_disabled"));
        }
        if item.kind == 7 && policy.mode != 3 {
            return (None, Some("mobile_requires_mode_3"));
        }
        if item.kind == 22 {
            if policy.mode < 2 {
                return (None, Some("mode_disabled"));
            }
            if control.road_category.is_none() && snapshot.source != Source::NaverV1 {
                return (None, Some("road_category_missing"));
            }
            if control.road_category.is_some_and(|c| c <= 1) {
                return (None, Some("road_category_blocked"));
            }
            if !policy.bump_speed_kph.is_finite() || policy.bump_speed_kph <= 0. {
                return (None, Some("invalid_item"));
            }
            return (
                Some(SafetyDecision {
                    provider: snapshot.source.name(),
                    reason: "bump",
                    limit_kph: policy.bump_speed_kph,
                    distance_m: item.distance_m,
                    kind: 22,
                    rejection: None,
                }),
                None,
            );
        }
        let (limit, distance, kind, reason) = if matches!(item.block_type, 2 | 3) {
            (item.block_speed_kph, item.block_distance_m, 4, "section")
        } else {
            (
                item.speed_limit_kph,
                item.distance_m,
                item.kind,
                if item.kind == 4 { "section" } else { "cam" },
            )
        };
        if !limit.is_finite() || limit <= 0. {
            return (None, Some("invalid_item"));
        }
        if !distance.is_finite() || distance <= 0. {
            return (None, Some("invalid_distance"));
        }
        (
            Some(SafetyDecision {
                provider: snapshot.source.name(),
                reason,
                limit_kph: limit * policy.safety_factor,
                distance_m: distance,
                kind,
                rejection: None,
            }),
            None,
        )
    };
    let (primary, primary_rejection) = decide(control.safety.as_ref());
    if primary.is_some() {
        return (primary, None);
    }
    let (secondary, secondary_rejection) = decide(control.secondary_safety.as_ref());
    if secondary.is_some() {
        return (secondary, None);
    }
    (
        None,
        primary_rejection.or(secondary_rejection).or(Some(
            if selection.safety_age_s.is_some() || selection.secondary_safety_age_s.is_some() {
                "safety_stale"
            } else {
                "safety_absent"
            },
        )),
    )
}

pub fn choose_safety(
    selection: &Selection,
    mut policy: SafetyPolicy,
    hda: (f64, f64),
) -> Option<SafetyDecision> {
    if !policy.safety_factor.is_finite() || policy.safety_factor <= 0. {
        policy.safety_factor = 1.;
    }
    let (navigation, rejection) = navigation_safety(selection, policy);
    navigation.or_else(|| {
        (hda.0.is_finite() && hda.1.is_finite() && hda.0 > 0. && hda.1 > 0.).then_some(
            SafetyDecision {
                provider: "hda",
                reason: "hda",
                limit_kph: hda.0 * policy.safety_factor,
                distance_m: hda.1,
                kind: -1,
                rejection,
            },
        )
    })
}
