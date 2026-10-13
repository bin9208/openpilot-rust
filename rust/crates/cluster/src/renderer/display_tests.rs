use super::display::*;

#[test]
fn language_aliases_and_source_text_fallback() {
    assert_eq!(normalize_language(" main_KOR ", "en"), "ko");
    assert_eq!(normalize_language("English", "ko"), "en");
    assert_eq!(normalize_language("unknown", "ko"), "ko");
    assert_eq!(text("ko", "driving_report"), "주행리포트");
    assert_eq!(text("en", "system_unresponsive"), "System Unresponsive");
    assert_eq!(text("ko", "unmapped"), "unmapped");
}

#[test]
fn source_distance_boundaries_units_and_rounding() {
    assert_eq!(format_navi_distance(999.5, true), "1000 m");
    assert_eq!(format_navi_distance(1000.0, true), "1.0 km");
    assert_eq!(format_navi_distance(10_000.0, true), "10 km");
    assert_eq!(format_navi_distance(160.0, false), "525 ft");
    assert_eq!(format_navi_distance(1609.344, false), "1.0 mi");
    assert_eq!(format_trip_distance(1050.0, true), "1.05 km");
    assert_eq!(format_radar_distance(2.5, true), "2 m");
    assert_eq!(format_radar_distance(-1.0, true), "0 m");
    assert_eq!(
        display_speed(f64::INFINITY, false).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(display_speed(f64::NAN, true).to_bits(), 0.0_f64.to_bits());
    assert_eq!(speed_unit(false), "mph");
}
