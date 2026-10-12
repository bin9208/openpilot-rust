use openpilot_cluster::autorun::{gpu::wait_for_usbgpu_startup, params::Reader};
use openpilot_params::Params;
use std::{
    io::{self, Read},
    path::PathBuf,
};

#[derive(serde::Deserialize)]
struct Case {
    root: PathBuf,
    prefix: String,
    actions: Vec<Action>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Settings,
    Initialize,
    RemoveNamespace,
    WaitGpu {
        expected: bool,
    },
    StartupExpected {
        repository: PathBuf,
        assets: PathBuf,
        sysfs: PathBuf,
    },
    WaitGpuFilesystem {
        repository: PathBuf,
        assets: PathBuf,
        sysfs: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let params = Params::open(&case.root, &case.prefix)?;
    let reader = Reader::new(params.clone());
    let mut observed = Vec::new();
    for action in case.actions {
        observed.push(match action {
            Action::Settings => serde_json::json!({
                "settings":reader.settings(),"allowed":reader.output_allowed(),
            }),
            Action::StartupExpected { repository, assets, sysfs } => {
                serde_json::json!(openpilot_cluster::autorun::gpu::startup_expected(&repository, &assets, &sysfs)?)
            }
            Action::WaitGpuFilesystem { repository, assets, sysfs } => {
                let mut probes = 0;
                wait_for_usbgpu_startup(&reader, || {
                    probes += 1;
                    openpilot_cluster::autorun::gpu::startup_expected(&repository, &assets, &sysfs)
                })?;
                serde_json::json!({"probes":probes})
            }
            Action::Initialize => {
                reader.initialize()?;
                serde_json::Value::Null
            }
            Action::RemoveNamespace => {
                std::fs::remove_dir_all(case.root.join(&case.prefix))?;
                serde_json::Value::Null
            }
            Action::WaitGpu { expected } => {
                println!("READY");
                std::io::Write::flush(&mut io::stdout())?;
                let started = std::time::Instant::now();
                let mut probes = 0;
                wait_for_usbgpu_startup(&reader, || {
                    probes += 1;
                    Ok(expected)
                })?;
                serde_json::json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"probes":probes})
            }
        });
    }
    drop(reader);
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "observed":observed,"connected":params.get_bool("ClusterHudConnected")?,
            "namespace_exists":case.root.join(&case.prefix).is_dir(),
        }))?
    );
    Ok(())
}
