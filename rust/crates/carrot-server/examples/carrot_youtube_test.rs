use openpilot_carrot_server::{
    youtube_test::{self, CommandSpec, Config, Paths},
    Value,
};
use std::path::{Path, PathBuf};

fn config(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let input = Value::parse(&std::fs::read_to_string(path)?)?;
    let root = PathBuf::from(input.get("owned_root").string()?);
    let params = PathBuf::from(input.get("params_root").string()?);
    if !params.starts_with(&root) {
        return Err("fixture Params root is not owned".into());
    }
    let children = |name: &str| -> Result<PathBuf, Box<dyn std::error::Error>> {
        let path = PathBuf::from(input.get(name).string()?);
        if !path.starts_with(&root) {
            return Err("fixture child is not owned".into());
        }
        Ok(path)
    };
    let camera = children("camera")?;
    let url = input.get("status_url").string()?;
    let parsed = url::Url::parse(&url)?;
    let host: std::net::IpAddr = parsed
        .host_str()
        .ok_or("fixture status host missing")?
        .parse()?;
    if !host.is_loopback() {
        return Err("fixture status recipient must be loopback".into());
    }
    let runner = std::env::current_exe()?;
    Ok(Config {
        params: openpilot_params::Params::open(&params, &input.get("prefix").string()?)?,
        repository: PathBuf::from(input.get("repository").string()?),
        paths: Paths {
            state: root.join("test-state.json"),
            log: root.join("test.log"),
            report: root.join("test-report.json"),
            live_state: root.join("state/youtube_live.json"),
            secret: root.join("state/youtube_live_secret.json"),
        },
        camera: CommandSpec {
            pattern: camera.to_string_lossy().into_owned(),
            path: camera,
            args: Vec::new(),
        },
        encoder: children("encoder")?,
        runner: CommandSpec {
            pattern: runner.to_string_lossy().into_owned(),
            path: runner,
            args: vec!["--config".into(), path.as_os_str().to_owned()],
        },
        launcher: PathBuf::from(input.get("launcher").string()?),
        status_url: url,
    })
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--config") {
        return Err("fixture needs --config".into());
    }
    let input = args.next().ok_or("fixture config path missing")?;
    let args: Vec<_> = args.collect();
    let code = youtube_test::run_command(&config(Path::new(&input))?, &args).await?;
    std::process::exit(code);
}
