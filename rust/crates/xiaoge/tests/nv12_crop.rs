use openpilot_xiaoge::nv12::{Error, Frame, Layout};

// 8x6 visible pixels, stride 10, four bytes between Y and UV, and a tail.
// Each padding/gap byte differs from the visible Y and interleaved UV bytes.
const PADDED: &[u8] = &[
    1, 2, 3, 4, 5, 6, 7, 8, 201, 202, // Y row 0
    9, 10, 11, 12, 13, 14, 15, 16, 203, 204, // Y row 1
    17, 18, 19, 20, 21, 22, 23, 24, 205, 206, // Y row 2
    25, 26, 27, 28, 29, 30, 31, 32, 207, 208, // Y row 3
    33, 34, 35, 36, 37, 38, 39, 40, 209, 210, // Y row 4
    41, 42, 43, 44, 45, 46, 47, 48, 211, 212, // Y row 5
    221, 222, 223, 224, // allocation gap
    101, 102, 103, 104, 105, 106, 107, 108, 231, 232, // UV row 0
    111, 112, 113, 114, 115, 116, 117, 118, 233, 234, // UV row 1
    121, 122, 123, 124, 125, 126, 127, 128, 235, 236, // UV row 2
    241, 242, // bytes beyond the complete UV plane
];

const LAYOUT: Layout = Layout {
    width: 8,
    height: 6,
    stride: 10,
    uv_offset: 64,
};

#[test]
fn full_crop_preserves_visible_y_then_interleaved_uv_order() {
    let frame = Frame::new(PADDED, LAYOUT).unwrap();
    let expected = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48,
        101, 102, 103, 104, 105, 106, 107, 108, 111, 112, 113, 114, 115, 116, 117, 118, 121, 122,
        123, 124, 125, 126, 127, 128,
    ];
    assert_eq!(frame.pack().unwrap(), expected);
    assert_eq!(frame.crop_nv12([0, 0, 8, 6]).unwrap(), expected);
    let exact_frame = Frame::new(&PADDED[..94], LAYOUT).unwrap();
    assert_eq!(exact_frame.crop_nv12([0, 0, 8, 6]).unwrap(), expected);
}

#[test]
fn interior_crop_skips_padding_gap_and_unselected_chroma_pairs() {
    let frame = Frame::new(PADDED, LAYOUT).unwrap();
    assert_eq!(
        frame.crop_nv12([2, 2, 4, 2]).unwrap(),
        [19, 20, 21, 22, 27, 28, 29, 30, 113, 114, 115, 116]
    );
}

#[test]
fn edge_crops_include_exactly_the_requested_y_and_uv_rows() {
    let frame = Frame::new(PADDED, LAYOUT).unwrap();
    for (bounds, expected) in [
        ([0, 0, 2, 2], &[1, 2, 9, 10, 101, 102][..]),
        ([6, 4, 2, 2], &[39, 40, 47, 48, 127, 128][..]),
        (
            [0, 2, 8, 2],
            &[
                17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 111, 112, 113, 114,
                115, 116, 117, 118,
            ][..],
        ),
        (
            [2, 0, 2, 6],
            &[
                3, 4, 11, 12, 19, 20, 27, 28, 35, 36, 43, 44, 103, 104, 113, 114, 123, 124,
            ][..],
        ),
    ] {
        assert_eq!(frame.crop_nv12(bounds).unwrap(), expected, "{bounds:?}");
    }
}

#[test]
fn tightly_packed_crop_uses_the_same_uv_pair_order() {
    let data = [1, 2, 3, 4, 5, 6, 7, 8, 101, 102, 103, 104];
    let frame = Frame::new(
        &data,
        Layout {
            width: 4,
            height: 2,
            stride: 4,
            uv_offset: 8,
        },
    )
    .unwrap();
    assert_eq!(
        frame.crop_nv12([2, 0, 2, 2]).unwrap(),
        [3, 4, 7, 8, 103, 104]
    );
}

#[test]
fn crop_preserves_support_for_odd_stride_and_a_separate_uv_gap() {
    let data = [
        1, 2, 3, 4, 201, 5, 6, 7, 8, 202, 221, 222, 101, 102, 103, 104, 231,
    ];
    let frame = Frame::new(
        &data,
        Layout {
            width: 4,
            height: 2,
            stride: 5,
            uv_offset: 12,
        },
    )
    .unwrap();
    assert_eq!(
        frame.crop_nv12([2, 0, 2, 2]).unwrap(),
        [3, 4, 7, 8, 103, 104]
    );
}

#[test]
fn crop_rejects_zero_size_and_out_of_frame_bounds() {
    let frame = Frame::new(PADDED, LAYOUT).unwrap();
    for bounds in [
        [0, 0, 0, 2],
        [0, 0, 2, 0],
        [8, 0, 2, 2],
        [0, 6, 2, 2],
        [6, 0, 4, 2],
        [0, 4, 2, 4],
        [0, 0, 10, 2],
        [0, 0, 2, 8],
    ] {
        assert!(frame.crop_nv12(bounds).is_err(), "{bounds:?}");
    }
}

#[test]
fn crop_rejects_origins_and_sizes_that_split_chroma_samples() {
    let frame = Frame::new(PADDED, LAYOUT).unwrap();
    for bounds in [[1, 0, 2, 2], [0, 1, 2, 2], [0, 0, 3, 2], [0, 0, 2, 3]] {
        assert!(frame.crop_nv12(bounds).is_err(), "{bounds:?}");
    }
}

#[test]
fn crop_bounds_addition_overflow_returns_an_error_instead_of_wrapping() {
    let frame = Frame::new(PADDED, LAYOUT).unwrap();
    for bounds in [
        [usize::MAX - 1, 0, 4, 2],
        [0, usize::MAX - 1, 2, 4],
        [2, 0, usize::MAX - 1, 2],
        [0, 2, 2, usize::MAX - 1],
    ] {
        assert_eq!(frame.crop_nv12(bounds), Err(Error::Overflow), "{bounds:?}");
    }
}

#[test]
fn even_crop_still_rejects_an_odd_full_frame_before_uv_validation() {
    for layout in [
        Layout {
            width: 7,
            uv_offset: 0,
            ..LAYOUT
        },
        Layout {
            height: 5,
            uv_offset: 0,
            ..LAYOUT
        },
    ] {
        let frame = Frame::new(PADDED, layout).unwrap();
        assert_eq!(frame.pack(), Err(Error::OddDimensions));
        assert_eq!(frame.crop_nv12([0, 0, 2, 2]), Err(Error::OddDimensions));
    }
}

#[test]
fn crop_still_rejects_uv_overlap_with_full_y_allocation() {
    // Offset 59 overlaps only the final Y row's padding. The requested ROI's
    // Y and UV bytes would fit, but pack() has always rejected this layout.
    let frame = Frame::new(
        PADDED,
        Layout {
            uv_offset: 59,
            ..LAYOUT
        },
    )
    .unwrap();
    assert_eq!(frame.pack(), Err(Error::Overlap));
    assert_eq!(frame.crop_nv12([0, 0, 2, 2]), Err(Error::Overlap));
}

#[test]
fn crop_requires_full_uv_plane_including_unselected_rows_and_final_padding() {
    // A top-left ROI can fit at length 66; every visible UV pixel fits at 92.
    // The original full-plane contract still requires all 94 bytes.
    for actual in [60, 66, 76, 92, 93] {
        let frame = Frame::new(&PADDED[..actual], LAYOUT).unwrap();
        let expected = Err(Error::ShortUv {
            actual,
            required: 94,
        });
        assert_eq!(frame.pack(), expected);
        assert_eq!(frame.crop_nv12([0, 0, 2, 2]), expected);
    }
}

#[test]
fn crop_preserves_full_uv_end_overflow_check() {
    let frame = Frame::new(
        PADDED,
        Layout {
            uv_offset: usize::MAX - 1,
            ..LAYOUT
        },
    )
    .unwrap();
    assert_eq!(frame.pack(), Err(Error::Overflow));
    assert_eq!(frame.crop_nv12([0, 0, 2, 2]), Err(Error::Overflow));
}
