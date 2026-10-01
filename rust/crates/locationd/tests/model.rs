use openpilot_locationd::model;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Sample {
    state: [f64; 18],
    dt: f64,
    functions: BTreeMap<String, Vec<f64>>,
}
#[test]
fn model_and_jacobians_match_actual_source_generated_functions(
) -> Result<(), Box<dyn std::error::Error>> {
    let samples: Vec<Sample> = serde_json::from_slice(include_bytes!("data/model.json"))?;
    for sample in samples {
        let x = &sample.state;
        let functions = [
            ("f_fun", model::transition(x, sample.dt).to_vec()),
            ("F_fun", model::transition_jacobian(x, sample.dt).to_vec()),
            ("h_4", model::observe_4(x).to_vec()),
            ("H_4", model::jacobian_4(x).to_vec()),
            ("h_10", model::observe_10(x).to_vec()),
            ("H_10", model::jacobian_10(x).to_vec()),
            ("h_13", model::observe_13(x).to_vec()),
            ("H_13", model::jacobian_13(x).to_vec()),
            ("h_14", model::observe_14(x).to_vec()),
            ("H_14", model::jacobian_14(x).to_vec()),
        ];
        for (name, actual) in functions {
            let expected = sample
                .functions
                .get(name)
                .ok_or("missing oracle function")?;
            assert_eq!(actual.len(), expected.len());
            for (index, (value, oracle)) in actual.iter().zip(expected).enumerate() {
                assert!(
                    (value - oracle).abs() <= 1e-9 + 1e-9 * oracle.abs(),
                    "{name}[{index}]: {value} != {oracle}"
                );
            }
        }
    }
    Ok(())
}
