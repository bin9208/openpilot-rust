use openpilot_paramsd::model;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Sample {
    state: [f64; 9],
    globals: [f64; 6],
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
            (
                "f_fun",
                model::transition(x, &sample.globals, sample.dt).to_vec(),
            ),
            (
                "F_fun",
                model::transition_jacobian(x, &sample.globals, sample.dt).to_vec(),
            ),
            ("h_24", model::observe_24(x).to_vec()),
            ("H_24", model::jacobian_24(x).to_vec()),
            ("h_25", model::observe_25(x).to_vec()),
            ("H_25", model::jacobian_25(x).to_vec()),
            ("h_26", model::observe_26(x).to_vec()),
            ("H_26", model::jacobian_26(x).to_vec()),
            ("h_27", model::observe_27(x).to_vec()),
            ("H_27", model::jacobian_27(x).to_vec()),
            ("h_28", model::observe_28(x).to_vec()),
            ("H_28", model::jacobian_28(x).to_vec()),
            ("h_29", model::observe_29(x).to_vec()),
            ("H_29", model::jacobian_29(x).to_vec()),
            ("h_30", model::observe_30(x).to_vec()),
            ("H_30", model::jacobian_30(x).to_vec()),
            ("h_31", model::observe_31(x).to_vec()),
            ("H_31", model::jacobian_31(x).to_vec()),
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
