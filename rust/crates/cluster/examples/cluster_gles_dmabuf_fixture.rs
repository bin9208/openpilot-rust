use openpilot_cluster::renderer::nv12::dmabuf::DmaTargetPool;
use serde_json::{json, Value};
use std::{fs::File, os::fd::AsFd, path::PathBuf};

fn fd_count() -> std::io::Result<usize> {
    Ok(std::fs::read_dir("/proc/self/fd")?.count())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, provider, output] = args.as_slice() else {
        return Err("cluster_gles_dmabuf_fixture PROVIDER OUTPUT".into());
    };
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let _window = if provider == "HOST" {
        Some(openpilot_startup_ui::renderer::Renderer::raw_window(
            (32, 24),
            "Owned DMA host fallback",
            128,
            0,
        )?)
    } else {
        None
    };
    let before = fd_count()?;
    let mut records = Vec::<Value>::new();
    let pool = if provider == "HOST" {
        DmaTargetPool::new()
    } else {
        DmaTargetPool::with_providers(provider, provider)
    };
    match pool {
        Err(error) => records.push(json!({"constructor_error":error.to_string()})),
        Ok(mut pool) => {
            let file = File::create(output.join("owned-buffer"))?;
            file.set_len(32)?;
            let first = pool.target_for(file.as_fd(), 8, 16).map(|view| view.info());
            match first {
                Err(error) => records.push(json!({"import_error":error.to_string()})),
                Ok(info) => {
                    let repeated = pool.target_for(file.as_fd(), 8, 16)?.info();
                    let changed = pool.target_for(file.as_fd(), 16, 32)?.info();
                    records.push(json!({"dimensions":[info.width,info.height],"cache_reused":info.framebuffer==repeated.framebuffer,"changed_dimensions":[changed.width,changed.height],"new_target":info.framebuffer!=changed.framebuffer}));
                    records.push(match pool.wait_for_gpu() {
                        Ok(()) => json!({"wait":true}),
                        Err(error) => json!({"wait_error":error.to_string()}),
                    });
                }
            }
            drop(file);
            pool.close()?;
            pool.close()?;
        }
    }
    records.push(json!({"fd_delta":i64::try_from(fd_count()?)?-i64::try_from(before)?}));
    println!("{}", serde_json::to_string(&records)?);
    Ok(())
}
