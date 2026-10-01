use super::*;
pub struct ApplicationConfig {
    pub graphics: Config,
    pub assets: PathBuf,
    pub language: String,
    pub title: String,
    pub dimensions: Option<(f32, f32)>,
    pub fps: i32,
    pub diagnostics: Options,
}

impl ApplicationConfig {
    pub fn for_runtime(root: &std::path::Path, title: &str) -> Result<Self, Error> {
        let (graphics, _) = Config::for_runtime()?;
        let params = openpilot_params::Params::for_runtime()
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        let translations = crate::multilang::Multilang::from_params(
            &root.join("openpilot/selfdrive/ui/translations"),
            &params,
        )
        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        Ok(Self {
            graphics,
            assets: root.join("openpilot/selfdrive/assets"),
            language: translations.language().into(),
            title: title.into(),
            dimensions: None,
            fps: 20,
            diagnostics: Options::from_environment()?,
        })
    }
}
