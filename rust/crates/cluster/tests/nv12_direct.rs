use openpilot_cluster::renderer::{
    nv12::{Nv12Layout, Nv12Packer},
    ClusterUiRenderer, Options,
};

#[test]
fn direct_destination_rejects_nonpacked_layout_without_opening_window() {
    let mut renderer = ClusterUiRenderer::new(std::path::Path::new("."), Options::default());
    let mut packer = Nv12Packer::default();
    let layout = Nv12Layout {
        width: 8,
        height: 4,
        stride: 13,
        y_scanlines: 4,
        uv_scanlines: 2,
        uv_offset: 52,
        byte_count: 78,
    };
    let error = renderer
        .render_to_nv12_target(
            &mut packer,
            layout,
            false,
            |_| panic!("incompatible layout reached capture"),
            |_| Ok(()),
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "direct NV12 readback requires a four-byte packed Venus layout"
    );
}
