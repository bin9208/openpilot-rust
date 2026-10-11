use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CameraKind {
    Road,
    WideRoad,
    Driver,
}

impl CameraKind {
    #[must_use]
    pub const fn service(self) -> &'static str {
        match self {
            Self::Road => "roadCameraState",
            Self::WideRoad => "wideRoadCameraState",
            Self::Driver => "driverCameraState",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CameraSpec {
    pub kind: CameraKind,
    pub id: String,
    pub device: String,
}

/// Preserve the original environment order and platform-specific device prefix.
#[must_use]
pub fn selected(mut getenv: impl FnMut(&str) -> Option<String>, darwin: bool) -> Vec<CameraSpec> {
    let mut cameras = vec![(
        CameraKind::Road,
        getenv("ROAD_CAM").unwrap_or_else(|| "0".into()),
    )];
    for (kind, variable) in [
        (CameraKind::WideRoad, "WIDE_CAM"),
        (CameraKind::Driver, "DRIVER_CAM"),
    ] {
        if let Some(value) = getenv(variable).filter(|value| !value.is_empty()) {
            cameras.push((kind, value));
        }
    }
    cameras
        .into_iter()
        .map(|(kind, id)| CameraSpec {
            device: if darwin {
                id.clone()
            } else {
                format!("/dev/video{id}")
            },
            kind,
            id,
        })
        .collect()
}

/// Read runtime camera IDs without replacing an invalid Unicode value with0.
///
/// # Errors
/// Returns the invalid environment value before any capture is opened.
pub fn environment(darwin: bool) -> Result<Vec<CameraSpec>, std::env::VarError> {
    let names = ["ROAD_CAM", "WIDE_CAM", "DRIVER_CAM"];
    let mut values = std::collections::HashMap::new();
    for name in names {
        match std::env::var(name) {
            Ok(value) => {
                values.insert(name, value);
            }
            Err(std::env::VarError::NotPresent) => {}
            Err(error @ std::env::VarError::NotUnicode(_)) => return Err(error),
        }
    }
    Ok(selected(|name| values.get(name).cloned(), darwin))
}

#[cfg(test)]
mod tests {
    use super::{selected, CameraKind};

    #[test]
    fn linux_prefixes_string_ids_and_preserves_optional_order() {
        let cameras = selected(
            |name| match name {
                "ROAD_CAM" => Some("/owned/road.avi".into()),
                "WIDE_CAM" => Some("wide".into()),
                "DRIVER_CAM" => Some("driver".into()),
                _ => None,
            },
            false,
        );
        assert_eq!(
            cameras.iter().map(|camera| camera.kind).collect::<Vec<_>>(),
            [CameraKind::Road, CameraKind::WideRoad, CameraKind::Driver]
        );
        assert_eq!(cameras[0].device, "/dev/video/owned/road.avi");
        assert_eq!(cameras[1].device, "/dev/videowide");
    }

    #[test]
    fn darwin_preserves_ids_while_empty_optional_values_are_absent() {
        let cameras = selected(|name| (name == "WIDE_CAM").then(String::new), true);
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].device, "0");
    }
}
