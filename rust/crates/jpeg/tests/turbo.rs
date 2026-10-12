#![cfg(feature = "native-skip-miri")]
use openpilot_jpeg::{Color, Layout, Options, Quality, TurboJpeg};
#[test]
fn selected_missing_turbo_provider_reports_error_before_pointer_use() {
    assert!(TurboJpeg::open(std::path::Path::new("/owned/missing/libturbojpeg.so")).is_err());
    let layout = Layout::new(2, 2, Color::Rgbx).unwrap();
    assert!(layout.check_pixels(&[0; 15]).is_err());
    assert!(Quality::new(0).is_err());
    let _options = Options::new(Quality::new(82).unwrap());
}
