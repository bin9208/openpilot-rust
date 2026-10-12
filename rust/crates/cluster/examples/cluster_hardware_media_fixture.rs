use openpilot_cluster::{
    media::{frame::FrameLease, HardwareFrame},
    models::NaviMediaFrame,
    renderer::{ClusterUiRenderer, MediaFit, Options, Rect},
};
use openpilot_startup_ui::egl::FrameLayout;
use std::{
    os::fd::OwnedFd,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
};

fn descriptors() -> Result<usize, std::io::Error> {
    Ok(std::fs::read_dir("/proc/self/fd")?.count())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, output] = args.as_slice() else {
        return Err("cluster_hardware_media_fixture ROOT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let mut renderer = open_renderer(&root)?;
    let rgba = NaviMediaFrame {
        key: "owned-hardware".into(),
        sequence: 1,
        present: true,
        mime: "image/rgba".into(),
        width: 8,
        height: 4,
        data: Some(Arc::from([180, 60, 20, 255].repeat(32))),
        ..NaviMediaFrame::default()
    };
    let rect = Rect::new(30.25, 18.75, 530.5, 270.25);
    let original = renderer.capture_draw(|renderer| {
        renderer.gfx.clear([0, 0, 0])?;
        assert!(renderer.draw_navi_media(Some(&rgba), rect, MediaFit::default())?);
        Ok(())
    })?;
    let baseline = descriptors()?;
    let mut failures = Vec::new();
    let mut releases = Vec::new();
    for index in 0..12 {
        let (release_tx, release_rx) = mpsc::channel();
        let (failure_tx, failure_rx) = mpsc::channel();
        let fd: OwnedFd = tempfile::tempfile()?.into();
        let buffer = HardwareFrame::retained(
            fd,
            FrameLayout {
                width: 8,
                height: 4,
                stride: 8,
                uv_offset: 32,
            },
            FrameLease {
                index,
                generation: u64::from(index / 4),
                sequence: u64::from(index + 2),
                release: release_tx,
                failures: failure_tx,
            },
        )?;
        let frame = NaviMediaFrame {
            key: rgba.key.clone(),
            sequence: u64::from(index + 2),
            present: true,
            mime: "video/nv12-dmabuf".into(),
            width: 8,
            height: 4,
            hardware_buffer: Some(Arc::clone(&buffer)),
            ..NaviMediaFrame::default()
        };
        let actual = renderer.capture_draw(|renderer| {
            renderer.gfx.clear([0, 0, 0])?;
            assert!(renderer.draw_navi_media(Some(&frame), rect, MediaFit::default())?);
            Ok(())
        })?;
        assert_eq!(
            actual.rgba, original.rgba,
            "failed hardware import must preserve the prior image"
        );
        assert_eq!(
            Arc::strong_count(&buffer),
            2,
            "EGL caches must not retain decoder frame leases"
        );
        let message = failure_rx.try_recv()?;
        assert_eq!(message, "EGL DMA-BUF import is unavailable");
        assert!(failure_rx.try_recv().is_err());
        assert!(release_rx.try_recv().is_err());
        failures.push(message);
        drop(frame);
        assert_eq!(Arc::strong_count(&buffer), 1);
        drop(buffer);
        let returned_index = release_rx.try_recv()?;
        assert_eq!(returned_index, index);
        releases.push(returned_index);
        assert_eq!(
            descriptors()?,
            baseline,
            "failed import leaked a descriptor"
        );
    }
    renderer.write_png(&original, &output.join("preserved-image.png"))?;
    renderer.close()?;
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames":12,"generations":3,"failures":failures,"releases":releases,"fd_delta":0,"fallback_pixels_equal":true,"lease_strong_count_during_draw":2}),
        )?,
    )?;
    Ok(())
}

fn open_renderer(root: &Path) -> Result<ClusterUiRenderer, openpilot_cluster::renderer::Error> {
    let mut renderer = ClusterUiRenderer::new(
        root,
        Options {
            width: 640,
            height: 480,
            theme_mode: "dark".into(),
            ..Options::default()
        },
    );
    renderer.open(true)?;
    Ok(renderer)
}
