use openpilot_ui_application::mici::onroad::debug_plot::samples::Samples;
use serde::Deserialize;
use std::io::Read;
#[derive(Deserialize)]
struct Step {
    now: f64,
    values: [f64; 3],
    #[serde(default)]
    reset: bool,
    #[serde(default)]
    history: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let steps: Vec<Step> = serde_json::from_str(&input)?;
    let mut samples = Samples::default();
    let mut result = Vec::new();
    for step in steps {
        if step.reset {
            samples = Samples::default();
        }
        samples.sample(step.now, step.values);
        let history = step.history.then(|| {
            std::array::from_fn::<_, 3, _>(|series| {
                (0..samples.size)
                    .map(|back| samples.value(series, back))
                    .collect::<Vec<_>>()
            })
        });
        result.push(serde_json::json!({"sample":samples.snapshot(),"history":history}));
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
