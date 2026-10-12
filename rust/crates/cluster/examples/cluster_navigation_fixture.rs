use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::live::navigation::{
    fresh_carrot_navi, parse_carrot_navi, resolve_navi_speed_limit,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Step {
    message: Option<PathBuf>,
    now: f64,
    base: Option<i64>,
    source: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input] = args.as_slice() else {
        return Err("cluster_navigation_fixture INPUT".into());
    };
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut state = None;
    let mut output = Vec::new();
    for step in steps {
        if let Some(path) = step.message {
            let bytes = std::fs::read(path)?;
            let message = serialize::read_message(&mut &bytes[..], ReaderOptions::new())?;
            let event::CarrotNavi(data) = message.get_root::<event::Reader<'_>>()?.which()? else {
                return Err("fixture carrotNavi missing".into());
            };
            state = parse_carrot_navi(data?.into(), step.now, state.as_ref());
        }
        let (fresh, deadline) = fresh_carrot_navi(state.as_ref(), step.now);
        state = fresh;
        let limit = resolve_navi_speed_limit(step.base, step.source.as_deref(), state.as_ref());
        output.push(serde_json::json!({"state": state, "deadline": deadline.is_finite().then_some(deadline), "limit": limit}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
