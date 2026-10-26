use openpilot_runtime_build::{generation::Config, selection, Result};
use std::{os::unix::process::CommandExt, path::PathBuf, process::Command};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut mode, mut root, mut config, mut id) = (None, None, None, None);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-native --config CONFIG_JSON (--build --root CHECKOUT | --activate ID | --resolve | --launch --root CHECKOUT)\nBuild/seal a full native AGNOS runtime with source-built manager and ABI tools; activation requires verified offroad state. Launch resolves an actual preserved source generation and reports fallback explicitly. Requires prepared, pinned toolchain/dependencies/provider inputs. No exact-commit runtime release is fetched.");
                return Ok(());
            }
            Some(value @ ("--build" | "--resolve" | "--launch")) if mode.is_none() => {
                mode = Some(value.to_owned())
            }
            Some("--activate") if mode.is_none() => {
                mode = Some("--activate".to_owned());
                id = Some(
                    args.next()
                        .ok_or("missing source generation ID")?
                        .into_string()
                        .map_err(|_| "invalid generation ID")?,
                );
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing source root")?))
            }
            Some("--config") if config.is_none() => {
                config = Some(PathBuf::from(args.next().ok_or("missing configuration")?))
            }
            _ => return Err("unknown, duplicate or mixed native runtime argument".into()),
        }
    }
    let config_path = config.ok_or("--config is required")?.canonicalize()?;
    let config = Config::read(&config_path)?;
    match mode.as_deref().ok_or("native runtime mode is required")? {
        "--build" => {
            let root = root
                .ok_or("--root is required for building")?
                .canonicalize()?;
            let id = config.build(&root)?;
            println!(
                "{}",
                serde_json::json!({"generation_id":id,"activated":false})
            );
        }
        "--activate" if root.is_none() => {
            let selected =
                selection::activate(&config.stores, &id.ok_or("missing generation ID")?)?;
            println!("{}", serde_json::to_string_pretty(&selected)?);
        }
        "--resolve" if root.is_none() => println!(
            "{}",
            serde_json::to_string_pretty(&selection::resolve(&config.stores)?)?
        ),
        "--launch" => {
            let root = root.ok_or("--root is required for launch")?;
            if !root.is_absolute()
                || root.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
            {
                return Err("requested launch checkout must be absolute".into());
            }
            let selected = selection::resolve(&config.stores)?;
            eprintln!(
                "{}",
                serde_json::json!({"requested_checkout":root,"selected":selected})
            );
            let error = Command::new(selected.source_root.join("rust/bin/openpilot-launch"))
                .arg("--root")
                .arg(&selected.source_root)
                .arg("--native-config")
                .arg(&config_path)
                .arg("--update-checkout")
                .arg(&root)
                .arg("--generation")
                .arg(&selected.id)
                .current_dir(&selected.source_root)
                .env("OPENPILOT_NATIVE_CONFIG", config_path)
                .env("OPENPILOT_UPDATE_CHECKOUT", root)
                .env("OPENPILOT_RUNNING_GENERATION", selected.id)
                .exec();
            return Err(error.into());
        }
        _ => return Err("native runtime mode does not accept these paths".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native runtime: {error}");
        std::process::exit(1);
    }
}
