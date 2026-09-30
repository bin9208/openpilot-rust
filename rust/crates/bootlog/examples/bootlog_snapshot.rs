use openpilot_bootlog::snapshot::save_bootlog;
use std::{
    io::{self, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("expected PARAMS_DIRECTORY LOGGERD_DIRECTORY MODE".into());
    }
    let result = save_bootlog(&PathBuf::from(&args[0]), &PathBuf::from(&args[1]));
    match result {
        Ok(worker) => {
            println!("{{\"phase\":\"returned\"}}");
            io::stdout().flush()?;
            if args[2] == "detached" {
                return Ok(());
            }
            let result = worker.join().map_err(|_| "snapshot worker panicked")?;
            println!(
                "{}",
                serde_json::json!({"phase":"worker_done","success":result.is_ok()})
            );
        }
        Err(_) => println!("{{\"phase\":\"copy_failed\"}}"),
    }
    Ok(())
}
