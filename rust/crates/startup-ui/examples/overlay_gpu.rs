use openpilot_startup_ui::{
    draw::Draw,
    geometry::Rect,
    overlay_cache::{Backend, Frame, Layer},
    renderer::Renderer,
    Error,
};
use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os().nth(1).ok_or("output JSON required")?;
    let mut renderer = Renderer::raw_window((320, 180), "native overlay regression", 128, 0)?;
    let mut results = Vec::new();
    for (width, height) in [(536, 240), (2160, 1080)] {
        for scale in [1.0f32, 0.5] {
            for recording in [false, true] {
                renderer.config.scale = scale;
                let size = (
                    (width as f32 * scale) as i32,
                    (height as f32 * scale) as i32,
                );
                let rotated = renderer.render_target(size)?;
                let recording_target = if recording {
                    Some(renderer.render_target(size)?)
                } else {
                    None
                };
                let target = recording_target.as_ref().unwrap_or(&rotated);
                let target_id = renderer.target_info(target)?.framebuffer;
                let mut layers = [Layer::new(0), Layer::new(1)];
                let mut paints = [0; 2];
                let mut hashes = Vec::new();
                let mut errors = Vec::new();
                for frame in 0..40 {
                    renderer.raw_begin()?;
                    renderer.begin_target(target)?;
                    renderer.push_matrix();
                    renderer.scale([scale, scale, 1.0]);
                    renderer.clear(u32::from_le_bytes([20 + (frame % 17) as u8, 40, 70, 255]))?;
                    renderer.scissor(Some(Rect {
                        x: 10.0,
                        y: 10.0,
                        width: width as f32 - 20.0,
                        height: height as f32 - 20.0,
                    }))?;
                    let urgent = [7, 8, 17].contains(&frame);
                    let mut failed = [false; 2];
                    for i in 0..2 {
                        let result = layers[i].draw(
                            &mut renderer,
                            [0.0, 0.0, width as f32, height as f32],
                            frame / 11,
                            Frame {
                                pixels: size,
                                logical: (width as f32, height as f32),
                                index: frame,
                                enabled: true,
                                urgent,
                            },
                            || 100.0 + frame as f64 * 0.04,
                            |r| {
                                paints[i] += 1;
                                let (x, tint) = if i == 0 {
                                    (width / 10, [230, 41, 55, 128])
                                } else {
                                    (width * 4 / 10, [0, 228, 48, 170])
                                };
                                r.rounded(
                                    Rect {
                                        x: x as f32,
                                        y: (height / 5) as f32,
                                        width: (width / 2) as f32,
                                        height: (height / 2) as f32,
                                    },
                                    0.0,
                                    u32::from_le_bytes(tint),
                                )?;
                                if frame == 22 && i == 0 {
                                    return Err(Error::Contract("owned painter error"));
                                }
                                Ok(())
                            },
                        );
                        failed[i] = result.is_err();
                        assert_eq!(
                            renderer.framebuffer(),
                            target_id,
                            "caller FBO lost at {frame}/{i}"
                        );
                    }
                    renderer.scissor(None)?;
                    renderer.pop_matrix();
                    renderer.end_target();
                    renderer.raw_end();
                    let pixels = renderer.target_pixels(target, true)?;
                    hashes.push(format!("{:x}", Sha256::digest(&pixels)));
                    errors.push(failed);
                }
                for layer in &mut layers {
                    layer.close(&mut renderer)?;
                }
                results.push(serde_json::json!({"width":width,"height":height,"scale":scale,"recording":recording,"paints":paints,"errors":errors,"hashes":hashes}));
                drop(recording_target);
                drop(rotated);
                renderer.release_resources()?;
            }
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}
