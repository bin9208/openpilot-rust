use openpilot_startup_ui::{
    camera::{CameraDraw, PlaneFormat, Style},
    config::Config,
    draw::Draw,
    geometry::Rect,
    renderer::{Renderer, Window},
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, output, width, height] = args.as_slice() else {
        return Err("cluster_camera_styles ROOT OUTPUT WIDTH HEIGHT".into());
    };
    let (width, height) = (width.parse::<f32>()?, height.parse::<f32>()?);
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let mut renderer = Renderer::new_window(
        Window {
            config: Config {
                big: false,
                large_viewport: false,
                pc: true,
                scale: 1.0,
            },
            dimensions: (width, height),
            title: "Camera style regression".into(),
            spinner: false,
            language: "en".into(),
        },
        &assets,
    )?;
    renderer.camera_shader(Style::Cluster, false)?;
    let y = renderer.camera_plane((16, 4), PlaneFormat::Luma)?;
    let uv = renderer.camera_plane((8, 2), PlaneFormat::Chroma)?;
    let bytes: Vec<_> = (32u8..128).collect();
    renderer.update_camera_plane(y.id(), &bytes[..64])?;
    renderer.update_camera_plane(uv.id(), &bytes[64..])?;
    let cases = [
        ("large", Style::Large),
        (
            "compact-00",
            Style::Compact {
                engaged: false,
                driver: false,
            },
        ),
        (
            "compact-10",
            Style::Compact {
                engaged: true,
                driver: false,
            },
        ),
        (
            "compact-01",
            Style::Compact {
                engaged: false,
                driver: true,
            },
        ),
        (
            "compact-11",
            Style::Compact {
                engaged: true,
                driver: true,
            },
        ),
        ("large-after-compact", Style::Large),
    ];
    for (name, style) in cases {
        renderer.begin();
        renderer.clear(u32::from_le_bytes([22, 33, 44, 255]))?;
        renderer.camera(CameraDraw {
            luma: y.id(),
            chroma: Some(uv.id()),
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 4.0,
            },
            destination: Rect {
                x: 7.25,
                y: 5.5,
                width: width - 22.5,
                height: height - 18.25,
            },
            style,
        })?;
        renderer.screenshot(&Path::new(output).join(format!("{name}.png")))?;
        renderer.end();
    }
    assert!(renderer.smooth_camera_plane(u32::MAX).is_err());
    drop(y);
    drop(uv);
    renderer.release_dynamic_textures();
    std::fs::write(
        Path::new(output).join("result.json"),
        b"{\"pass\":true,\"cases\":6,\"invalid_resource_rejected\":true}",
    )?;
    Ok(())
}
