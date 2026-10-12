use openpilot_carrot_recovery::{config::Config, upload, Error};
use std::path::PathBuf;

fn main() -> Result<(), Error> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| Error::Source("fixture root required".into()))?,
    );
    let reason = args
        .next()
        .ok_or_else(|| Error::Source("upload reason required".into()))?;
    let config = Config {
        repository: root.clone(),
        web: root.join("web"),
        launcher: root.join("unused-launcher"),
        data: root.join("data"),
        params: root.join("params"),
        tmux_log: root.join("tmux.log"),
    };
    println!(
        "{}",
        upload::destinations(&config, &reason.to_string_lossy())?.encode()?
    );
    Ok(())
}
