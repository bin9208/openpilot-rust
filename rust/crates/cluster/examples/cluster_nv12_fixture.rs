use openpilot_cluster::renderer::{
    nv12::{readback::DirectReadback, Nv12Layout, Nv12Packer, Nv12Request},
    ClusterUiRenderer, Options, Rect,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Request {
    groups: Vec<Group>,
}
#[derive(Deserialize)]
struct Group {
    width: i32,
    height: i32,
    theme: String,
    #[serde(default)]
    mode: Option<String>,
    cases: Vec<Case>,
}

fn paint(
    renderer: &mut ClusterUiRenderer,
    dimensions: (i32, i32),
    phase: u8,
) -> openpilot_cluster::renderer::Result<()> {
    renderer.gfx.clear(renderer.current_theme().bg)?;
    for row in 0_u8..4 {
        for column in 0_u8..8 {
            let red = (column * 31).wrapping_add(phase);
            let green = (row * 61).wrapping_add(phase.wrapping_mul(3));
            let blue = ((column + row) * 21).wrapping_add(phase.wrapping_mul(5));
            renderer.gfx.rectangle(
                Rect::new(
                    f64::from(i32::from(column) * dimensions.0) / 8.0,
                    f64::from(i32::from(row) * dimensions.1) / 4.0,
                    f64::from(dimensions.0) / 8.0,
                    f64::from(dimensions.1) / 4.0,
                ),
                [red, green, blue, 255],
            )?;
        }
    }
    renderer
        .gfx
        .rectangle(Rect::new(3.25, 7.5, 11.25, 9.5), [180, 30, 210, 127])?;
    Ok(())
}
#[derive(Deserialize)]
struct Case {
    name: String,
    layout: Nv12Layout,
    flip_x: bool,
    phase: u8,
    seed: Option<u8>,
    reuse: bool,
}

fn pack_case(
    renderer: &mut ClusterUiRenderer,
    packer: &mut Nv12Packer,
    case: &Case,
    dimensions: (i32, i32),
    method: &str,
    buffer: Vec<u8>,
    readback: Option<&mut DirectReadback>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let draw = |renderer: &mut ClusterUiRenderer| paint(renderer, dimensions, case.phase);
    Ok(match (method, case.layout.direct_packing()) {
        ("external", Some(packing)) => {
            let target = renderer
                .gfx
                .backend()?
                .render_target((packing.width, packing.height))?;
            let info = renderer.gfx.backend()?.target_info(&target)?;
            renderer.render_to_nv12_external(packer, case.layout, case.flip_x, info, draw)?;
            renderer.gfx.backend()?.target_pixels(&target, false)?
        }
        ("direct" | "async", Some(_)) => {
            let readback = readback.ok_or("missing readback")?;
            let mut bytes = vec![0; case.layout.byte_count];
            renderer.render_to_nv12_target(packer, case.layout, case.flip_x, draw, |info| {
                if method == "direct" {
                    readback.read_rgba(info, &mut bytes).map_err(|error| {
                        openpilot_cluster::renderer::Error::Io(std::io::Error::other(error))
                    })?;
                } else if !readback.enqueue_rgba(info, 100.0).map_err(|error| {
                    openpilot_cluster::renderer::Error::Io(std::io::Error::other(error))
                })? {
                    return Err(openpilot_cluster::renderer::Error::Contract(
                        "owned PBO ring rejected first frame",
                    ));
                }
                Ok(())
            })?;
            if method == "async" {
                let started = std::time::Instant::now();
                while !readback.async_ready(100.0 + started.elapsed().as_secs_f64())? {
                    if started.elapsed().as_secs_f64() > 1.0 {
                        return Err("owned PBO did not signal".into());
                    }
                    std::thread::yield_now();
                }
                if !readback.copy_ready(&mut bytes, 100.0 + started.elapsed().as_secs_f64())? {
                    return Err("owned ready PBO disappeared".into());
                }
            }
            bytes
        }
        _ => renderer.render_to_nv12_buffer(
            packer,
            Nv12Request {
                layout: case.layout,
                buffer,
                flip_x: case.flip_x,
            },
            draw,
        )?,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input, output] = args.as_slice() else {
        return Err("cluster_nv12_fixture ROOT INPUT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut records = Vec::new();
    for group in request.groups {
        let mut renderer = ClusterUiRenderer::new(
            &root,
            Options {
                width: group.width,
                height: group.height,
                theme_mode: group.theme,
                ..Options::default()
            },
        );
        let mut packer = Nv12Packer::default();
        renderer.open(true)?;
        let mut readback = match group.mode.as_deref() {
            Some("direct" | "async") => Some(DirectReadback::new()?),
            _ => None,
        };
        let mut previous = Vec::new();
        for case in group.cases {
            let buffer = if case.reuse {
                std::mem::take(&mut previous)
            } else {
                case.seed
                    .map_or_else(Vec::new, |seed| vec![seed; case.layout.byte_count])
            };
            let method = if case.layout.direct_packing().is_some() {
                group.mode.as_deref().unwrap_or("staged")
            } else {
                "staged"
            };
            let frame_bytes = pack_case(
                &mut renderer,
                &mut packer,
                &case,
                (group.width, group.height),
                method,
                buffer,
                readback.as_mut(),
            )?;
            let path = output.join(format!("{}.nv12", case.name));
            std::fs::write(&path, &frame_bytes)?;
            records.push(serde_json::json!({"name":case.name,"bytes":frame_bytes.len(),"path":path,"layout":case.layout,"method":method}));
            previous = frame_bytes;
        }
        if let Some(mut readback) = readback {
            readback.close()?;
        }
        drop(packer);
        renderer.close()?;
    }
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"cases":records.len(),"records":records}))?,
    )?;
    Ok(())
}
