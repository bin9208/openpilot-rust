use openpilot_dev_tools::plot_samples_source::render;
use openpilot_log_format::JsonValue;
use serde_json::Value;
use std::io::Read;

fn corpus() -> Value {
    let mut plain = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/plot-samples/original.json.gz").as_slice(),
    )
    .read_to_string(&mut plain)
    .unwrap();
    serde_json::from_str(&plain).unwrap()
}

#[test]
fn original_fixed_5000_steps_and_53_complete_history_boundaries() {
    let cases = corpus();
    let input = JsonValue::parse(cases["fixed"]["input"].as_str().unwrap()).unwrap();
    let expected = JsonValue::parse(cases["fixed"]["source"].as_str().unwrap()).unwrap();
    let actual = render(&input).unwrap();
    assert_eq!(actual.to_json().unwrap(), expected.to_json().unwrap());
}

#[test]
fn generic_original_sampling_conversion_reset_wrap_and_failure_semantics() {
    let cases = corpus();
    let cases = cases["generic"].as_array().unwrap();
    assert_eq!(cases.len(), 108);
    for case in cases {
        let input = JsonValue::parse(case["input"].as_str().unwrap()).unwrap();
        let actual = render(&input);
        if let Some(expected) = case["output"].as_str() {
            let expected = JsonValue::parse(expected).unwrap();
            assert_eq!(
                actual.unwrap().to_json().unwrap(),
                expected.to_json().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let error = actual.err().expect("original failure must remain failure");
            assert_eq!(
                error.kind,
                case["error"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(
                error.message,
                case["message"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
