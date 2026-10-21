#![cfg(feature = "native-skip-miri")]

use openpilot_opencv_runtime::{Dimensions, Format, Image, ImageLayout};
use openpilot_xiaoge::inference::lane_tensor;

#[test]
fn three_channels_keep_original_division_bits_for_all_gray_values() {
    let gray: Vec<u8> = (0..416 * 416).map(|index| (index % 256) as u8).collect();
    let image = Image::from_parts(
        ImageLayout::new(Dimensions::new(416, 416).unwrap(), Format::Gray).unwrap(),
        gray.clone(),
    )
    .unwrap();
    let tensor = lane_tensor(&image).unwrap();
    assert_eq!(tensor.view().shape(), [1, 3, 416, 416]);
    for channel in tensor.view().values().chunks_exact(gray.len()) {
        for (&value, &byte) in channel.iter().zip(&gray) {
            assert_eq!(value.to_bits(), (f32::from(byte) / 255.0).to_bits());
        }
    }
}
