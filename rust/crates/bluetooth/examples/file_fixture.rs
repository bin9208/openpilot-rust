use openpilot_bluetooth::{atomic_value, Error};
use openpilot_logmessaged::JsonValue;
use std::{
    fs,
    io::{self, BufRead},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("runtime directory argument required")?,
    );
    fs::create_dir_all(&root)?;
    for (index, line) in io::stdin().lock().lines().enumerate() {
        let path = root.join(format!("{index}.json"));
        fs::write(&path, b"initial")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
        let value = JsonValue::parse(&line?)?;
        let succeeded = match atomic_value(&path, &value) {
            Ok(()) => true,
            Err(Error::Encoding(_)) => false,
            Err(error) => return Err(error.into()),
        };
        println!("{succeeded}");
    }
    Ok(())
}
