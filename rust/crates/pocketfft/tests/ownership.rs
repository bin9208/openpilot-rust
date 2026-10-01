#![cfg(feature = "native-skip-miri")]
use openpilot_pocketfft::{Complex, Transform};
#[test]
fn checked_lengths_repeated_transform_and_ownership() {
    assert!(Transform::new(0).is_err());
    for n in [1, 2, 7, 11, 40, 200, 1225] {
        let mut fft = Transform::new(n).unwrap();
        assert!(fft.execute(&mut [], 1., true).is_err());
        let mut values = vec![Complex::default(); n];
        values[0].re = 1.;
        for _ in 0..4 {
            fft.execute(&mut values, 1., true).unwrap();
            assert!(values
                .iter()
                .all(|value| (value.re - 1.).abs() < 1e-12 && value.im.abs() < 1e-12));
            let scale = 1. / f64::from(u32::try_from(n).unwrap());
            fft.execute(&mut values, scale, false).unwrap();
            assert!((values[0].re - 1.).abs() < 1e-12);
            assert!(values[1..]
                .iter()
                .all(|value| value.re.abs() < 1e-12 && value.im.abs() < 1e-12));
        }
    }
}
