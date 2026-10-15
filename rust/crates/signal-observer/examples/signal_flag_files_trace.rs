use openpilot_signal_observer::{auto_runtime, backend::Request, runtime::Config};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Read},
};
#[derive(Deserialize)]
struct Case {
    kind: String,
    data: Vec<u8>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let root = env::temp_dir().join(format!("signal-flags-native-{}", std::process::id()));
    fs::create_dir(&root)?;
    let mut output = vec![];
    for (index, case) in cases.into_iter().enumerate() {
        let directory = root.join(index.to_string());
        fs::create_dir(&directory)?;
        for name in ["enabled", "neural_backend"] {
            match case.kind.as_str() {
                "directory" => fs::create_dir(directory.join(name))?,
                "value" => fs::write(directory.join(name), &case.data)?,
                "missing" => {}
                _ => return Err("invalid fixture kind".into()),
            }
        }
        let config = Config {
            directory,
            ..Config::default()
        };
        let flag = match auto_runtime::flag(&config, "enabled") {
            Ok(value) => json!({"value":value}),
            Err(_) => json!({"error":true}),
        };
        let backend = match auto_runtime::selection(&config) {
            Ok(request) => {
                json!({"value":match request {Request::Cpu=>"cpu",Request::Qcom=>"qcom",Request::Auto=>"auto"}})
            }
            Err(_) => json!({"error":true}),
        };
        output.push(json!({"flag":flag,"backend":backend}));
    }
    fs::remove_dir_all(root)?;
    println!("{}", serde_json::to_string(&Value::Array(output))?);
    Ok(())
}
