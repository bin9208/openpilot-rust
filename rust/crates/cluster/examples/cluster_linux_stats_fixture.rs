use openpilot_cluster::renderer::linux_stats::LinuxStatsReader;
use std::path::PathBuf;

#[derive(serde::Deserialize)]
struct Request {
    steps: Vec<Step>,
}
#[derive(serde::Deserialize)]
struct Step {
    memory: Option<Vec<u8>>,
    cpu: Option<Vec<u8>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input, output] = args.as_slice() else {
        return Err("cluster_linux_stats_fixture INPUT OUTPUT".into());
    };
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut reader = LinuxStatsReader::new(&output);
    let mut snapshots = Vec::new();
    for step in request.steps {
        for (name, data) in [("meminfo", step.memory), ("stat", step.cpu)] {
            let path = output.join(name);
            if let Some(data) = data {
                std::fs::write(path, data)?;
            } else if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        let sample = reader.sample()?.map(|mut value| {
            value.disk_used_percent = None;
            value
        });
        snapshots.push(sample);
    }
    println!("{}", serde_json::to_string(&snapshots)?);
    Ok(())
}
