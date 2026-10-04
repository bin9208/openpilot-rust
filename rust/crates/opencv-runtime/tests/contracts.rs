use openpilot_opencv_runtime::{
    Dimensions, Format, Image, ImageLayout, ImageView, Tensor, TensorView,
};

#[test]
fn dimensions_and_borrowed_image_lengths_are_checked_before_ffi() {
    assert!(Dimensions::new(0, 4).is_err());
    assert!(Dimensions::new(4, 0).is_err());
    assert!(Dimensions::new(u32::MAX, 4).is_err());
    let size = Dimensions::new(3, 2).unwrap();
    let layout = ImageLayout::new(size, Format::Rgb).unwrap();
    assert_eq!(layout.len(), 18);
    assert!(ImageView::new(&[0; 17], layout).is_err());
    assert!(ImageView::new(&[0; 19], layout).is_err());
    let source = [42; 18];
    let view = ImageView::new(&source, layout).unwrap();
    assert_eq!(view.data(), &source);
    assert_eq!(view.layout(), layout);
    let owned = Image::from_parts(layout, source.to_vec()).unwrap();
    assert_eq!(owned.view().data(), &source);
}

#[test]
fn nv12_requires_even_extent_and_exact_packed_planes() {
    assert!(Dimensions::new(3, 4).unwrap().nv12_len().is_err());
    assert!(Dimensions::new(4, 3).unwrap().nv12_len().is_err());
    assert_eq!(Dimensions::new(4, 2).unwrap().nv12_len().unwrap(), 12);
}

#[test]
fn tensor_rank_positive_axes_and_product_match_owned_and_borrowed_data() {
    assert!(TensorView::new(&[], &[1.]).is_err());
    assert!(TensorView::new(&[0, 3], &[]).is_err());
    assert!(TensorView::new(&[-1, 3], &[]).is_err());
    assert!(TensorView::new(&[1; 33], &[0.]).is_err());
    assert!(TensorView::new(&[i32::MAX; 32], &[]).is_err());
    assert!(TensorView::new(&[1, 3, 2], &[0.; 5]).is_err());
    assert!(Tensor::from_parts(vec![1, 3, 2], vec![0.; 7]).is_err());
    let shape = [1, 3, 2];
    let values = [1., 2., 3., 4., 5., 6.];
    let view = TensorView::new(&shape, &values).unwrap();
    assert_eq!(view.shape(), &shape);
    assert_eq!(view.values(), &values);
    let owned = Tensor::from_parts(shape.to_vec(), values.to_vec()).unwrap();
    assert_eq!(owned.view().shape(), &shape);
    assert_eq!(owned.view().values(), &values);
}
