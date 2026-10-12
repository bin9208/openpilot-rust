use super::{Error, FontConfig, Result};
use std::{
    env,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GuiConfig {
    pub sdl_library: String,
    pub ttf_library: String,
    pub tcl_library: String,
    pub tk_library: String,
    pub fonts: FontConfig,
    pub last_folder_file: PathBuf,
}
impl GuiConfig {
    /// # Errors
    /// Returns a non-Unicode provider environment value or missing home directory.
    pub fn installed(bin: &Path) -> Result<Self> {
        Ok(Self {
            sdl_library: provider("CLUSTER_REPLAY_SDL2_LIBRARY", "libSDL2-2.0.so.0")?,
            ttf_library: provider("CLUSTER_REPLAY_TTF_LIBRARY", "libSDL2_ttf-2.0.so.0")?,
            tcl_library: provider("CLUSTER_REPLAY_TCL_LIBRARY", "libtcl8.6.so")?,
            tk_library: provider("CLUSTER_REPLAY_TK_LIBRARY", "libtk8.6.so")?,
            fonts: FontConfig::installed(bin),
            last_folder_file: PathBuf::from(
                env::var_os("HOME").ok_or(Error::Contract("replay home directory missing"))?,
            )
            .join(".cluster_replay_usb_last_folder"),
        })
    }
}
fn provider(key: &str, default: &str) -> Result<String> {
    match env::var(key) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.into()),
        Err(env::VarError::NotUnicode(_)) => Err(Error::Contract("non-Unicode replay provider")),
    }
}
#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub program: PathBuf,
    pub launcher: PathBuf,
    pub cwd: PathBuf,
    pub gui: GuiConfig,
}
