use super::Result;
use std::path::Path;
/// # Errors
/// Returns invalid UTF-8 in the saved source folder. File I/O remains best effort.
pub fn load_last_folder(path: &Path) -> Result<String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let value = String::from_utf8(bytes)
                .map_err(|_| super::Error::Contract("last folder is not UTF-8"))?;
            let folder = value.trim_matches(openpilot_startup_ui::text::whitespace);
            Ok(if Path::new(folder).is_dir() {
                folder.into()
            } else {
                String::new()
            })
        }
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("replay last folder read: {error}");
            }
            Ok(String::new())
        }
    }
}
pub fn save_last_folder(path: &Path, folder: &str) {
    if let Err(error) = std::fs::write(path, folder) {
        eprintln!("replay last folder save: {error}");
    }
}
