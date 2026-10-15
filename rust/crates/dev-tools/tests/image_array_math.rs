use openpilot_dev_tools::image_array_math::compare;
use serde_json::Value;
#[test]
fn independent_numpy_broadcast_integer_overflow_and_axis_controls() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/image-array-math/original.json")).unwrap();
    for case in original["cases"].as_array().unwrap() {
        let shape = |key| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
                .collect::<Vec<_>>()
        };
        let values = |key| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect::<Vec<_>>()
        };
        let result = compare(
            &shape("left_shape"),
            &values("left"),
            &shape("right_shape"),
            &values("right"),
        );
        match case.get("expected") {
            Some(expected) => assert_eq!(
                serde_json::to_value(
                    result.unwrap_or_else(|error| panic!("{}: {error}", case["name"]))
                )
                .unwrap(),
                *expected,
                "{}",
                case["name"]
            ),
            None => assert!(result.is_err(), "{}: {result:?}", case["name"]),
        }
        assert!(
            case["warnings"].as_array().unwrap().is_empty(),
            "source integer math warning {}",
            case["name"]
        );
    }
}
