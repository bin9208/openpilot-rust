use super::{ClusterUiRenderer, Result, TextureView};
use crate::config::GREEN;
use openpilot_startup_ui::{
    draw::PixelBuffer,
    renderer::{DecodedImage, TextureFilter},
};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StaticImage {
    SpeedBackground,
    TrafficRed,
    TrafficGreen,
    FollowVehicle,
    Lfa,
    LfaActive,
    LfaLane,
    Wifi,
    TpmsCar,
}
#[derive(Clone, Copy)]
pub(super) enum ImageGroup {
    Follow,
    Status,
}
impl ClusterUiRenderer {
    #[must_use]
    pub fn static_texture(&self, kind: StaticImage) -> Option<TextureView> {
        self.static_images.get(&kind).map(Self::texture_view)
    }
    #[must_use]
    pub fn tpms_car_texture(&self) -> Option<TextureView> {
        self.static_texture(StaticImage::TpmsCar)
    }
    pub(super) fn load_static_images(&mut self, group: ImageGroup) -> Result<()> {
        for (kind, path, label) in [
            (
                StaticImage::FollowVehicle,
                "openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png",
                "Follow gap vehicle",
            ),
            (
                StaticImage::SpeedBackground,
                "openpilot/selfdrive/assets/images/speed_bg.png",
                "Speed background",
            ),
            (
                StaticImage::TrafficRed,
                "openpilot/selfdrive/assets/images/traffic_red.png",
                "Red traffic light",
            ),
            (
                StaticImage::TrafficGreen,
                "openpilot/selfdrive/assets/images/traffic_green.png",
                "Green traffic light",
            ),
            (
                StaticImage::Lfa,
                "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
                "LFA",
            ),
            (
                StaticImage::LfaActive,
                "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
                "LFA active",
            ),
            (
                StaticImage::LfaLane,
                "openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png",
                "LFA lane mode",
            ),
            (
                StaticImage::Wifi,
                "openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png",
                "Wi-Fi",
            ),
            (
                StaticImage::TpmsCar,
                "openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png",
                "TPMS toy car",
            ),
        ] {
            if matches!(kind, StaticImage::FollowVehicle) != matches!(group, ImageGroup::Follow)
                || self.static_images.contains_key(&kind)
            {
                continue;
            }
            let path = self.root.join(path);
            if !path.exists() {
                continue;
            }
            let image = decode(&path, kind);
            let decoded = match image {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(error) => {
                    eprintln!("{label} icon load failed: {error}");
                    continue;
                }
            };
            let texture = self.gfx.backend()?.dynamic_filtered_image(
                PixelBuffer {
                    dimensions: (decoded.width, decoded.height),
                    rgba: &decoded.rgba,
                },
                TextureFilter::Bilinear,
            );
            match texture {
                Ok(texture) => {
                    self.static_images.insert(kind, texture);
                }
                Err(error) => eprintln!("{label} icon load failed: {error}"),
            }
        }
        Ok(())
    }
}
fn decode(path: &Path, kind: StaticImage) -> Result<Option<DecodedImage>> {
    let mut image = DecodedImage::load(path)?;
    if kind == StaticImage::LfaActive {
        for pixel in image.rgba.chunks_exact_mut(4) {
            if pixel[3] > 0 && pixel[..3].iter().all(|value| *value >= 220) {
                pixel[..3].copy_from_slice(&GREEN);
            }
        }
    }
    if !matches!(kind, StaticImage::TrafficRed | StaticImage::TrafficGreen) {
        return Ok(Some(image));
    }
    let width =
        usize::try_from(image.width).map_err(|_| super::Error::Contract("icon width invalid"))?;
    let height =
        usize::try_from(image.height).map_err(|_| super::Error::Contract("icon height invalid"))?;
    let mut bounds = (width, height, 0, 0);
    let mut found = false;
    for (index, pixel) in image.rgba.chunks_exact(4).enumerate() {
        if pixel[3] > 8 {
            let (x, y) = (index % width, index / width);
            bounds = (
                bounds.0.min(x),
                bounds.1.min(y),
                bounds.2.max(x),
                bounds.3.max(y),
            );
            found = true;
        }
    }
    if !found {
        return Ok(None);
    }
    let left = bounds.0.saturating_sub(2);
    let top = bounds.1.saturating_sub(2);
    let right = (bounds.2 + 2).min(width - 1);
    let bottom = (bounds.3 + 2).min(height - 1);
    let mut rgba = Vec::with_capacity((right - left + 1) * (bottom - top + 1) * 4);
    for row in top..=bottom {
        rgba.extend_from_slice(
            &image.rgba[(row * width + left) * 4..(row * width + right + 1) * 4],
        );
    }
    image.width = i32::try_from(right - left + 1)
        .map_err(|_| super::Error::Contract("cropped width invalid"))?;
    image.height = i32::try_from(bottom - top + 1)
        .map_err(|_| super::Error::Contract("cropped height invalid"))?;
    image.rgba = rgba;
    Ok(Some(image))
}
