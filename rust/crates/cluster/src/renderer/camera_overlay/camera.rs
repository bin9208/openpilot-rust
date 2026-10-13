use crate::models::ClusterUiState;

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct CameraOverlayCamera {
    pub width: f64,
    pub height: f64,
    pub focal_length: f64,
}
const AR: [CameraOverlayCamera; 2] = [
    CameraOverlayCamera {
        width: 1928.,
        height: 1208.,
        focal_length: 2648.,
    },
    CameraOverlayCamera {
        width: 1928.,
        height: 1208.,
        focal_length: 567.,
    },
];
const OS: [CameraOverlayCamera; 2] = [
    CameraOverlayCamera {
        width: 1344.,
        height: 760.,
        focal_length: 1141.5,
    },
    CameraOverlayCamera {
        width: 1344.,
        height: 760.,
        focal_length: 425.25,
    },
];
const NEO: [CameraOverlayCamera; 2] = [CameraOverlayCamera {
    width: 1164.,
    height: 874.,
    focal_length: 910.,
}; 2];
// Preserve DEVICE_CAMERAS insertion order for the sensor-only fallback.
const CAMERAS: [(&str, &str, [CameraOverlayCamera; 2]); 14] = [
    ("neo", "unknown", NEO),
    ("tici", "unknown", AR),
    ("unknown", "ar0231", AR),
    ("unknown", "ox03c10", AR),
    ("pc", "unknown", AR),
    ("tici", "ar0231", AR),
    ("tici", "ox03c10", AR),
    ("tici", "os04c10", OS),
    ("tizi", "ar0231", AR),
    ("tizi", "ox03c10", AR),
    ("tizi", "os04c10", OS),
    ("mici", "ar0231", AR),
    ("mici", "ox03c10", AR),
    ("mici", "os04c10", OS),
];
#[must_use]
pub fn camera_for_state(state: &ClusterUiState, wide: bool) -> CameraOverlayCamera {
    let normalize = |value: Option<&str>| {
        value
            .unwrap_or("")
            .trim_matches(openpilot_startup_ui::text::whitespace)
            .to_lowercase()
    };
    let device = normalize(state.camera_device_type.as_deref());
    let sensor = normalize(state.camera_sensor.as_deref());
    for key in [
        (device.as_str(), sensor.as_str()),
        ("unknown", sensor.as_str()),
        (device.as_str(), "unknown"),
    ] {
        if let Some((_, _, cameras)) = CAMERAS
            .iter()
            .find(|(device, sensor, _)| (*device, *sensor) == key)
        {
            return cameras[usize::from(wide)];
        }
    }
    if !sensor.is_empty() {
        if let Some((_, _, cameras)) = CAMERAS
            .iter()
            .find(|(_, known_sensor, _)| *known_sensor == sensor)
        {
            return cameras[usize::from(wide)];
        }
    }
    AR[usize::from(wide)]
}
