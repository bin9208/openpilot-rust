use openpilot_lagd::{
    blocks::BlockAverage, correlation::next_good_size, estimate::parabolic, smoothing,
};
#[test]
fn padding_and_boundary_peaks() {
    assert_eq!(next_good_size(1220).unwrap(), 1225);
    assert_eq!(next_good_size(13).unwrap(), 14);
    assert_eq!(parabolic(&[1., 0., 0.], 0).unwrap(), 0.);
    assert_eq!(parabolic(&[0., 1., 0.], 1).unwrap(), 1.);
    assert_eq!(parabolic(&[0., 0., 1.], 2).unwrap(), 2.);
}
#[test]
fn masks_remain_missing_without_neighbors() {
    let result = smoothing::masked(&[2.; 8], &[false; 8], (5, 1.)).unwrap();
    assert!(result.iter().all(|value| value.is_nan()));
    assert!(smoothing::masked(&[1.], &[true], (4, 1.)).is_err());
}
#[test]
fn full_ring_excludes_the_current_block_before_the_next_write() {
    let mut blocks = BlockAverage::new(2, 2, (0.3, 0)).unwrap();
    blocks.update(0.4).unwrap();
    blocks.update(0.4).unwrap();
    assert_eq!(blocks.statistics().unwrap().valid_mean, 0.4);
    blocks.update(0.8).unwrap();
    blocks.update(0.8).unwrap();
    assert_eq!(blocks.valid_blocks, 2);
    assert_eq!(blocks.statistics().unwrap().valid_mean, 0.8);
}
