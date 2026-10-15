use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    values: Vec<u64>,
    result: u64,
}
#[test]
fn compensated_norm_matches_cpython_finite_bits_subnormals_and_infinite_precedence() {
    let bytes =
        zstd::stream::decode_all(include_bytes!("fixtures/python-norm-final.json.zst").as_slice())
            .unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.len(), 4105);
    for (i, case) in cases.iter().enumerate() {
        let values: Vec<_> = case.values.iter().map(|&v| f64::from_bits(v)).collect();
        let actual = openpilot_radarcan::scalar::norm(&values);
        let expected = f64::from_bits(case.result);
        if expected.is_nan() {
            assert!(actual.is_nan(), "norm/{i}");
        } else {
            assert_eq!(
                actual.to_bits(),
                case.result,
                "norm/{i}: {actual} != {expected}"
            );
        }
    }
}
