mod state_fixture;
use openpilot_pandad::state::Publisher;
use serde_json::json;
use state_fixture::{Fixture, Request};
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let mut publisher = Publisher::default();
        let mut results = Vec::with_capacity(request.steps.len());
        let mut exit = false;
        for step in request.steps {
            let mut fixture = Fixture {
                identities: request.identities.clone(),
                step,
                exit,
                actions: Vec::new(),
            };
            let ignition = publisher.update(fixture.step.input, &mut fixture)?;
            exit = fixture.exit;
            results.push(json!({"ignition":ignition,"exit":exit,"actions":fixture.actions}));
        }
        println!("{}", serde_json::to_string(&results)?);
    }
    Ok(())
}
