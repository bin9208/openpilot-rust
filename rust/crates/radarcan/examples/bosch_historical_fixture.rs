//! Explicit cross-environment historical diagnostic. The regular regression
//! suite compares every field against the same-environment original instead.
#[path = "../tests/support/bosch_full_state.rs"]
mod support;
use serde_json::Value;

fn main() {
    support::check(
        serde_json::Deserializer::from_reader(std::io::BufReader::new(support::frozen_fixture()))
            .into_iter::<Value>()
            .map(Result::unwrap),
    );
}
