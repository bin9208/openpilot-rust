//! Real graphics regression for recording toggles during widget rendering.
use openpilot_startup_ui::{draw::Draw, geometry::Rect, renderer::Renderer};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = Renderer::raw_window((64, 48), "recording boundary", 128, 0)?;
    renderer.begin();
    renderer.ensure_render_target((64, 48))?;
    renderer.rounded(
        Rect {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 48.,
        },
        0.,
        u32::from_le_bytes([90, 120, 150, 255]),
    )?;
    renderer.end();
    assert!(
        renderer.capture_frame_pixels().is_err(),
        "newly allocated target was not selected at frame start"
    );
    renderer.begin();
    renderer.rounded(
        Rect {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 48.,
        },
        0.,
        u32::from_le_bytes([90, 120, 150, 255]),
    )?;
    renderer.end();
    let pixels = renderer.capture_frame_pixels()?;
    assert_eq!(pixels.len(), 64 * 48 * 4);
    assert!(pixels.chunks_exact(4).all(|p| p == [90, 120, 150, 255]));
    renderer.begin();
    assert!(
        renderer.release_render_target().is_err(),
        "active target must survive until EndDrawing"
    );
    renderer.end();
    renderer.release_render_target()?;
    assert!(!renderer.has_render_target() && !renderer.has_capture_frame());
    // Cluster/external capture is deliberately a separate readback boundary.
    renderer.ensure_render_target((64, 48))?;
    renderer.begin_capture()?;
    renderer.clear(u32::from_le_bytes([1, 2, 3, 255]))?;
    renderer.end_capture();
    assert!(renderer
        .capture_pixels()?
        .chunks_exact(4)
        .all(|p| p == [1, 2, 3, 255]));
    println!("PASS recording target selected at frame boundary and actual RGBA readback");
    Ok(())
}
