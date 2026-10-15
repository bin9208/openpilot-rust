use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::{net::UnixStream, process::CommandExt},
    process::{Command, ExitCode},
};

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let executable = env::current_exe()?;
    let name = executable
        .file_name()
        .ok_or("missing executable name")?
        .to_str()
        .ok_or("invalid executable name")?;
    let args: Vec<_> = env::args_os().skip(1).collect();
    if name == "openpilot-process-child" {
        openpilot_process_supervision::run_child(File::open(
            args.first().ok_or("missing child request")?,
        )?)?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut record = OpenOptions::new()
        .create(true)
        .append(true)
        .open(env::var_os("OWNED_BOOTSTRAP_RECORD").ok_or("missing owned record")?)?;
    let lock = env::var_os("CARROT_REPO_LOCK_PATH").ok_or("missing lock path")?;
    let inherited = fs::read_dir("/proc/self/fd")?
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .any(|path| path.as_os_str() == lock);
    let libraries = env::var_os("LD_LIBRARY_PATH").map_or_else(Vec::new, |value| {
        env::split_paths(&value).collect::<Vec<_>>()
    });
    record.write_all(format!("{}\n", serde_json::json!({"role":name,"args":args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),"pid":std::process::id(),"inherited_lock":inherited,"library_path":libraries,"startup_failed":env::var("CARROT_BIG_MODEL_STARTUP_FAILED").ok(),"model_lock_held":env::var_os("CARROT_REPO_LOCK_PATH").and_then(|p|std::path::PathBuf::from(p).parent().map(|p|p.join("big_model_update.lock"))).and_then(|p|File::open(p).ok()).is_some_and(|f| f.try_lock().is_err())})).as_bytes())?;
    match name {
        "git" => {
            if args.iter().any(|arg| arg == "--format=%ct") {
                println!("1");
            } else {
                return Ok(ExitCode::FAILURE);
            }
        }
        "openpilot-manager" => {
            let mut stream = UnixStream::connect(
                env::var_os("CARROT_BOOT_LOCK_SOCKET").ok_or("missing boot socket")?,
            )?;
            let received = openpilot_process_supervision::receive_boot_lock(&mut stream)?;
            let other = File::open(&lock)?;
            if other.try_lock().is_ok() {
                return Err("manager received an unlocked repository".into());
            }
            received.unlock()?;
            other.try_lock()?;
            record.write_all(b"{\"role\":\"manager-lock-released\"}\n")?;
            if env::var("OWNED_EGPU").as_deref() == Ok("manager-failed") {
                for index in 0..1200 {
                    println!("owned diagnostic {index:04} {}", "x".repeat(64));
                }
                eprint!("Error: owned native manager failure without newline");
                std::io::stderr().flush()?;
                return Ok(ExitCode::from(7));
            }
        }
        "openpilot-agnos" => {
            return Ok(if env::var("OWNED_VERIFY").as_deref() == Ok("ready") {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        "taskset" => {
            if args.len() < 4 {
                return Err("invalid taskset fixture arguments".into());
            }
            return Err(Command::new(&args[2]).args(&args[3..]).exec().into());
        }
        "openpilot-usbgpu-model" => {
            if args.first().is_some_and(|a| a == "--present") {
                return Ok(
                    if env::var("OWNED_EGPU").is_ok_and(|v| v.starts_with("egpu-")) {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    },
                );
            }
            if args.first().is_some_and(|a| a == "--prepare-for-startup") {
                let cache = std::path::PathBuf::from(
                    env::var_os("CARROT_BIG_MODEL_DIR").ok_or("missing owned model cache")?,
                );
                fs::create_dir_all(&cache)?;
                fs::write(
                    cache.join("status.json"),
                    r#"{"schema_version":1,"state":"waiting_for_network","error_code":"dns"}"#,
                )?;
                std::thread::sleep(std::time::Duration::from_millis(
                    if env::var("OWNED_EGPU").as_deref() == Ok("egpu-cancel-delivery") {
                        30000
                    } else {
                        350
                    },
                ));
                if env::var("OWNED_EGPU").as_deref() == Ok("egpu-failed") {
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        "openpilot-spinner" => {
            let mut line = String::new();
            while std::io::stdin().read_line(&mut line)? > 0 {
                record.write_all(
                    format!(
                        "{}\n",
                        serde_json::json!({"role":"spinner-line","text":line})
                    )
                    .as_bytes(),
                )?;
                line.clear();
            }
        }
        "openpilot-carrot-recovery"
        | "openpilot-carrot-web-watchdog"
        | "openpilot-text-window"
        | "openpilot-startup-recovery"
        | "openpilot-agnos-updater"
        | "sudo"
        | "tmux" => {}
        _ => return Err(format!("unexpected provider {name}").into()),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("owned bootstrap provider: {error}");
            ExitCode::FAILURE
        }
    }
}
