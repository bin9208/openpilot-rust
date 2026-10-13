use openpilot_cluster::renderer::nv12::Nv12Layout;

#[test]
fn source_venus_layout_and_direct_pack_offsets_are_preserved() {
    let layout = Nv12Layout::venus(480, 1920).unwrap();
    assert_eq!(
        (layout.stride, layout.y_scanlines, layout.uv_scanlines),
        (512, 1920, 960)
    );
    assert_eq!((layout.uv_offset, layout.byte_count), (983_040, 1_757_184));
    let packing = layout.direct_packing().unwrap();
    assert_eq!((packing.width, packing.height), (128, 3432));
    assert_eq!((packing.uv_y, packing.y_y), (552, 1512));
    println!("native source Venus layout {layout:?}, packed target {packing:?}");
    for (width, height, stride, y_scanlines, uv_scanlines, uv_offset, byte_count) in [
        (96, 64, 128, 64, 32, 8192, 294_912),
        (64, 96, 128, 96, 48, 12_288, 303_104),
        (384, 960, 384, 960, 480, 368_640, 835_584),
        (1280, 720, 1280, 736, 368, 942_080, 2_220_032),
    ] {
        assert_eq!(
            Nv12Layout::venus(width, height).unwrap(),
            Nv12Layout {
                width,
                height,
                stride,
                y_scanlines,
                uv_scanlines,
                uv_offset,
                byte_count
            }
        );
    }
}

#[test]
fn malformed_and_overflowing_nv12_layouts_are_rejected_before_gpu_allocation() {
    let valid = Nv12Layout {
        width: 8,
        height: 4,
        stride: 12,
        y_scanlines: 4,
        uv_scanlines: 2,
        uv_offset: 48,
        byte_count: 72,
    };
    assert!(valid.validate().is_ok());
    for invalid in [
        Nv12Layout { width: 0, ..valid },
        Nv12Layout { stride: 7, ..valid },
        Nv12Layout {
            y_scanlines: 3,
            ..valid
        },
        Nv12Layout {
            uv_scanlines: 1,
            ..valid
        },
        Nv12Layout {
            uv_offset: 47,
            ..valid
        },
        Nv12Layout {
            byte_count: 71,
            ..valid
        },
        Nv12Layout {
            stride: i32::MAX,
            y_scanlines: i32::MAX,
            uv_scanlines: i32::MAX,
            ..valid
        },
    ] {
        assert!(
            invalid.validate().is_err(),
            "accepted malformed layout {invalid:?}"
        );
    }
    assert!(Nv12Layout::venus(i32::MAX, i32::MAX).is_err());
    assert!(Nv12Layout {
        stride: 13,
        uv_offset: 52,
        byte_count: 78,
        ..valid
    }
    .direct_packing()
    .is_none());
}
