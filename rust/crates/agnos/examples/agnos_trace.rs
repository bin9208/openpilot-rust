use openpilot_agnos::{
    cache,
    cli::{self, Config, Mode},
    decompress::Decompressor,
    image,
    manifest::{self, Partition},
    runtime::{self, NativeCommands},
    Error, Observer,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[derive(Deserialize)]
struct Request {
    config: Config,
    manifest: PathBuf,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Helpers,
    Verify {
        partition: Partition,
        force: bool,
    },
    Flash {
        standalone: bool,
        retry_network: bool,
    },
    Cache {
        partition: Partition,
    },
    Compressed {
        partition: Partition,
    },
    Casync {
        partition: Partition,
    },
    Clear {
        partition: Partition,
    },
    Swap,
    Execute {
        mode: Mode,
        retry_network: bool,
    },
    Decompress {
        path: PathBuf,
        reads: Vec<usize>,
    },
    Target,
}
#[derive(Default)]
struct Events(Vec<Value>);
impl Observer for Events {
    fn log(&mut self, level: &str, text: &str) {
        self.0.push(json!(["log", level, text]));
    }
    fn progress(&mut self, stage: &str, progress: i64) {
        self.0
            .push(json!(["progress", stage, progress.clamp(0, 100)]));
    }
    fn sleep(&mut self, seconds: u64) {
        self.0.push(json!(["sleep", seconds]));
    }
}
fn operation(
    request: &Request,
    op: &Operation,
    events: &mut Events,
    commands: &mut NativeCommands,
) -> Result<Value, Error> {
    let paths = &request.config.paths;
    Ok(match op {
        Operation::Helpers => {
            let before = manifest::confirmed(paths, &request.manifest)?;
            let urls = manifest::download_urls(&request.manifest)?;
            manifest::mark_confirmed(paths, &request.manifest)?;
            let after = manifest::confirmed(paths, &request.manifest)?;
            let content = std::fs::read_to_string(&paths.confirmation)?;
            manifest::unlink_if_present(&paths.confirmation)?;
            json!({"before":before,"after":after,"content":content,"urls":urls,"cleared":!paths.confirmation.exists()})
        }
        Operation::Verify { partition, force } => {
            json!(image::verify(paths, 1, partition, *force)?)
        }
        Operation::Flash {
            standalone,
            retry_network,
        } => {
            runtime::flash(
                paths,
                &request.manifest,
                1,
                *standalone,
                *retry_network,
                commands,
                events,
            )?;
            Value::Null
        }
        Operation::Cache { partition } => json!(cache::download(paths, partition, events)?
            .map(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))),
        Operation::Compressed { partition } => {
            image::compressed(paths, 1, partition, events)?;
            Value::Null
        }
        Operation::Casync { partition } => {
            openpilot_agnos::casync::extract_image(paths, 1, partition, events)?;
            Value::Null
        }
        Operation::Clear { partition } => {
            image::clear_hash(paths, 1, partition)?;
            Value::Null
        }
        Operation::Swap => {
            runtime::swap(paths, &request.manifest, 1, commands, events)?;
            Value::Null
        }
        Operation::Execute {
            mode,
            retry_network,
        } => json!(cli::execute(
            paths,
            &request.manifest,
            1,
            *mode,
            *retry_network,
            commands,
            events
        )?),
        Operation::Target => json!(runtime::target_slot(commands)?),
        Operation::Decompress { path, reads } => {
            let mut source = Decompressor::new("unused", Some(path))?;
            let mut results = Vec::new();
            for &size in reads {
                let data = source.read(size)?;
                results.push(
                    json!({"length":data.len(),"sha256":format!("{:x}",Sha256::digest(&data))}),
                );
            }
            json!({"reads":results,"sha256":source.hash_hex()})
        }
    })
}
fn main() -> Result<(), Error> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let mut commands = NativeCommands {
        abctl: request.config.abctl.clone(),
        launcher: request.config.launcher.clone(),
    };
    let mut rows = Vec::new();
    let mut events = Events::default();
    for op in &request.operations {
        let result = match operation(&request, op, &mut events, &mut commands) {
            Ok(v) => json!({"result":v}),
            Err(Error::Request {
                class, transient, ..
            }) => json!({"request":class,"transient":transient}),
            Err(e) => json!({"error":e.to_string()}),
        };
        rows.push(result);
    }
    serde_json::to_writer(std::io::stdout(), &json!({"rows":rows,"events":events.0}))?;
    Ok(())
}
