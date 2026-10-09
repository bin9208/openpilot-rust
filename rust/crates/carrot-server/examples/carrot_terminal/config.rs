use openpilot_carrot_server::{
    terminal_commands, vision_test, web_settings::WebSettings, youtube_test::CommandSpec, Value,
};
use std::path::{Path, PathBuf};

pub(super) fn configuration(
    path: &Path,
) -> Result<(terminal_commands::Config, vision_test::Config), Box<dyn std::error::Error>> {
    let input = Value::parse(&std::fs::read_to_string(path)?)?;
    let root = PathBuf::from(input.get("owned_root").string()?);
    let params = PathBuf::from(input.get("params_root").string()?);
    let owned_config = path.canonicalize()?;
    let owned_root = root.canonicalize()?;
    if owned_config.parent() != Some(owned_root.as_path())
        || !params.starts_with(&root)
        || root == Path::new("/")
    {
        return Err("fixture roots are not owned".into());
    }
    let child = |name: &str| -> Result<CommandSpec, Box<dyn std::error::Error>> {
        let path = PathBuf::from(input.get(name).string()?);
        if !path.starts_with(&root) {
            return Err("fixture child is not owned".into());
        }
        Ok(CommandSpec {
            pattern: path.to_string_lossy().into_owned(),
            path,
            args: Vec::new(),
        })
    };
    let binary = std::env::current_exe()?;
    let runner = CommandSpec {
        pattern: format!(
            "{}\0--config\0{}\0--vision-run",
            binary.display(),
            path.display()
        ),
        path: binary,
        args: vec![
            "--config".into(),
            path.as_os_str().into(),
            "--vision-run".into(),
        ],
    };
    let vision = vision_test::Config {
        repository: root.join("repository"),
        state: root.join("vision-state.json"),
        log: root.join("vision.log"),
        params_root: Some(params),
        launcher: PathBuf::from(input.get("launcher").string()?),
        runner,
        children: [
            ("camerad".into(), child("camera")?),
            ("stream_encoderd".into(), child("encoder")?),
            ("webrtcd".into(), child("webrtc")?),
        ],
        port: input.get("port").int()?.try_into()?,
    };
    let commands = terminal_commands::Config {
        web: WebSettings::new(
            &root.join("state/web_settings.json"),
            &root.join("content-catalog.json"),
        ),
        vision: Some(vision.clone()),
        youtube: None,
    };
    Ok((commands, vision))
}
