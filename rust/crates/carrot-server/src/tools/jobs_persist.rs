use super::jobs::Store;
use crate::{Error, Value};
use std::io::Write;

impl Store {
    pub fn persist(&self) {
        let write = || -> Result<(), Error> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let value = Value::object([
                ("version", Value::integer(1)),
                ("updated_at", Value::Float((self.clock)())),
                ("jobs", self.snapshots(20)?),
            ]);
            let temporary = std::path::PathBuf::from(format!("{}.tmp", self.path.display()));
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(crate::state_json::compact_encoded(&value.encode()?).as_bytes())?;
            drop(file);
            std::fs::rename(temporary, &self.path)?;
            *self
                .last_persist
                .lock()
                .map_err(|_| Error::Source("Tools persistence lock poisoned".into()))? =
                (self.clock)();
            Ok(())
        };
        if let Err(error) = write() {
            eprintln!("Tools history persistence: {error}");
        }
    }
    pub(super) fn persist_changed(&self) {
        let last = self.last_persist.lock().map(|last| *last).unwrap_or(0.);
        if (self.clock)() - last >= 0.5 {
            self.persist();
        }
    }
}
