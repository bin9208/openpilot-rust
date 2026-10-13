pub const KPH_TO_MPH: f64 = 0.621_371_192_237_334;
pub const METERS_TO_FEET: f64 = 3.280_839_895_013_123;
pub const METERS_PER_MILE: f64 = 1609.344;

#[must_use]
pub fn normalize_language(value: &str, default: &str) -> &'static str {
    let normalized = value
        .trim_matches(openpilot_startup_ui::text::whitespace)
        .to_lowercase();
    let normalized = normalized.strip_prefix("main_").unwrap_or(&normalized);
    match normalized {
        "ko" | "kr" | "kor" | "korean" => "ko",
        "en" | "eng" | "english" => "en",
        _ if default == "ko" => "ko",
        _ => "en",
    }
}

#[must_use]
pub fn text<'a>(language: &str, key: &'a str) -> &'a str {
    match (normalize_language(language, "en"), key) {
        ("en", "driving_report") => "DRIVING REPORT",
        ("en", "trip_summary") => "TRIP SUMMARY",
        ("en", "time") => "TIME",
        ("en", "distance") => "DISTANCE",
        ("en", "average_speed") => "AVG SPEED",
        ("en", "max_speed") => "MAX SPEED",
        ("en", "auto_drive") => "AUTO DRIVE",
        ("en", "max_accel") => "MAX ACCEL",
        ("en", "max_decel") => "MAX DECEL",
        ("en", "hard_accel") => "HARD ACCEL",
        ("en", "hard_brake") => "HARD BRAKE",
        ("en", "hard_corner") => "HARD CORNER",
        ("en", "system") => "SYSTEM",
        ("en", "device_angle") => "DEVICE ANGLE",
        ("en", "driving_mode_eco") => "ECO",
        ("en", "driving_mode_safe") => "SAFE",
        ("en", "driving_mode_normal") => "NORMAL",
        ("en", "driving_mode_sport") => "SPORT",
        ("en", "navigation") => "NAVIGATION",
        ("en", "next") => "NEXT",
        ("en", "section") => "SECTION",
        ("en", "average") => "AVG",
        ("en", "route") => "ROUTE",
        ("en", "connected") => "CONNECTED",
        ("en", "waiting") => "WAITING",
        ("en", "off_route") => "OFF ROUTE",
        ("en", "openpilot_unavailable") => "openpilot Unavailable",
        ("en", "waiting_to_start") => "Waiting to start",
        ("en", "take_control_immediately") => "TAKE CONTROL IMMEDIATELY",
        ("en", "system_unresponsive") => "System Unresponsive",
        ("en", "reboot_device") => "Reboot Device",
        ("ko", "driving_report") => "주행리포트",
        ("ko", "trip_summary") => "주행 요약",
        ("ko", "time") => "시간",
        ("ko", "distance") => "거리",
        ("ko", "average_speed") => "평균속도",
        ("ko", "max_speed") => "최고속도",
        ("ko", "auto_drive") => "자동주행",
        ("ko", "max_accel") => "최대가속",
        ("ko", "max_decel") => "최대감속",
        ("ko", "hard_accel") => "급가속",
        ("ko", "hard_brake") => "급감속",
        ("ko", "hard_corner") => "급코너",
        ("ko", "system") => "시스템",
        ("ko", "device_angle") => "디바이스 설치각",
        ("ko", "driving_mode_eco") => "연비",
        ("ko", "driving_mode_safe") => "안전",
        ("ko", "driving_mode_normal") => "일반",
        ("ko", "driving_mode_sport") => "고속",
        ("ko", "navigation") => "내비게이션",
        ("ko", "next") => "다음",
        ("ko", "section") => "구간",
        ("ko", "average") => "평균",
        ("ko", "route") => "경로",
        ("ko", "connected") => "연결됨",
        ("ko", "waiting") => "연결 대기",
        ("ko", "off_route") => "경로 이탈",
        ("ko", "openpilot_unavailable") => "openpilot 사용 불가",
        ("ko", "waiting_to_start") => "시작 대기 중",
        ("ko", "take_control_immediately") => "즉시 운전대를 잡으세요",
        ("ko", "system_unresponsive") => "시스템 응답 없음",
        ("ko", "reboot_device") => "기기를 재부팅하세요",
        _ => key,
    }
}

#[must_use]
pub fn display_speed(speed_kph: f64, is_metric: bool) -> f64 {
    if !speed_kph.is_finite() {
        return 0.0;
    }
    if is_metric {
        speed_kph
    } else {
        speed_kph * KPH_TO_MPH
    }
}

#[must_use]
pub fn speed_unit(is_metric: bool) -> &'static str {
    if is_metric {
        "km/h"
    } else {
        "mph"
    }
}

fn nonnegative(distance: f64) -> f64 {
    if distance > 0.0 {
        distance
    } else {
        0.0
    }
}

#[must_use]
pub fn format_radar_distance(distance_m: f64, is_metric: bool) -> String {
    let distance = nonnegative(distance_m);
    if is_metric {
        format!("{distance:.0} m")
    } else {
        format!("{:.0} ft", distance * METERS_TO_FEET)
    }
}

#[must_use]
pub fn format_navi_distance(distance_m: f64, is_metric: bool) -> String {
    let distance = nonnegative(distance_m);
    if is_metric {
        if distance < 1000.0 {
            return format!("{:.0} m", distance.round_ties_even());
        }
        if distance < 10_000.0 {
            return format!("{:.1} km", distance / 1000.0);
        }
        return format!("{:.0} km", distance / 1000.0);
    }
    let miles = distance / METERS_PER_MILE;
    if miles < 0.1 {
        format!("{:.0} ft", distance * METERS_TO_FEET)
    } else if miles < 10.0 {
        format!("{miles:.1} mi")
    } else {
        format!("{miles:.0} mi")
    }
}

#[must_use]
pub fn format_trip_distance(distance_m: f64, is_metric: bool) -> String {
    let distance = nonnegative(distance_m);
    if is_metric {
        if distance < 1000.0 {
            return format!("{distance:.0} m");
        }
        if distance < 10_000.0 {
            return format!("{:.2} km", distance / 1000.0);
        }
        return format!("{:.1} km", distance / 1000.0);
    }
    let miles = distance / METERS_PER_MILE;
    if miles < 0.1 {
        format!("{:.0} ft", distance * METERS_TO_FEET)
    } else if miles < 10.0 {
        format!("{miles:.2} mi")
    } else {
        format!("{miles:.1} mi")
    }
}
