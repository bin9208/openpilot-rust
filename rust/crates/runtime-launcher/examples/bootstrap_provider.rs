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
    record.write_all(format!("{}\n", serde_json::json!({"role":name,"args":args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),"pid":std::process::id(),"inherited_lock":inherited,"library_path":libraries})).as_bytes())?;
    match name {
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
        "openpilot-carrot-recovery"
        | "openpilot-carrot-web-watchdog"
        | "openpilot-text-window"
        | "openpilot-agnos-updater"
        | "openpilot-usbgpu-model"
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
