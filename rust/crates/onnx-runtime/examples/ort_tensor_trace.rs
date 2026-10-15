//! Real pinned model execution from frozen actual-source tensor bytes.
use openpilot_onnx_runtime::{Provider, Session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let provider = Provider::verified(&PathBuf::from(args.next().ok_or("ORT root required")?))?;
    let models = PathBuf::from(args.next().ok_or("model root required")?);
    let output = PathBuf::from(args.next().ok_or("output directory required")?);
    fs::create_dir(&output)?;
    let fixture: Value =
        serde_json::from_str(include_str!("../../../upstream/signal-runtime.json"))?;
    let mut sessions = BTreeMap::new();
    for name in ["detector", "classifier"] {
        sessions.insert(
            name,
            Session::load(
                &provider,
                &models.join(format!("{name}.onnx")),
                fixture["models"][name]["sha256"]
                    .as_str()
                    .ok_or("missing model hash")?,
            )?,
        );
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let source: Value = serde_json::from_str(&input)?;
    let mut records = Vec::new();
    for (index, run) in source["tensors"]
        .as_array()
        .ok_or("missing tensors")?
        .iter()
        .enumerate()
    {
        let name = run["network"].as_str().ok_or("missing network")?;
        let bytes = fs::read(run["input_path"].as_str().ok_or("missing tensor path")?)?;
        if bytes.len() % 4 != 0 {
            return Err("unaligned Float32 tensor".into());
        }
        if format!("{:x}", Sha256::digest(&bytes))
            != run["input_sha256"].as_str().ok_or("missing input hash")?
        {
            return Err("input tensor identity mismatch".into());
        }
        let values = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().expect("exact chunk")))
            .collect::<Vec<_>>();
        let shape: Vec<i64> = serde_json::from_value(run["input_shape"].clone())?;
        let tensors = sessions.get_mut(name).ok_or("unknown network")?.run(
            run["input_name"].as_str().ok_or("missing input name")?,
            &values,
            &shape,
        )?;
        let mut outputs = Vec::new();
        for (n, tensor) in tensors.into_iter().enumerate() {
            let bytes = tensor
                .values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>();
            let path = output.join(format!("{index}-output-{n}.f32"));
            fs::write(&path, &bytes)?;
            outputs.push(json!({"shape":tensor.shape,"path":fs::canonicalize(path)?,"sha256":format!("{:x}",Sha256::digest(&bytes))}));
        }
        records.push(json!({"network":name,"outputs":outputs}));
    }
    println!("{}", serde_json::to_string(&records)?);
    Ok(())
}
