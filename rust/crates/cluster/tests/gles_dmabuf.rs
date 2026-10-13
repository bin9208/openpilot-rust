use openpilot_cluster::renderer::nv12::dmabuf::PackedLayout;

#[test]
fn packed_dma_layout_rejects_partial_rows() {
    for (stride, size) in [(0, 16), (3, 12), (4, 0), (8, 15)] {
        assert!(PackedLayout::new(stride, size).is_err());
    }
}

#[test]
fn packed_dma_layout_matches_venus_rgba_geometry() {
    assert_eq!(
        PackedLayout::new(512, 1_757_184).unwrap().dimensions(),
        (128, 3432)
    );
}
