use crate::Error;
use openpilot_can::dbc::Dbc;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

pub fn parse(name: &str, text: &str, emit: &mut impl FnMut(&str)) -> Result<Arc<Dbc>, Error> {
    emit(&format!("DBC: {name}\n"));
    if name.starts_with("hyundai_canfd_generated") {
        emit("Using Hyundai CAN FD checksum\n");
    }
    Ok(Arc::new(Dbc::parse(name, text)?))
}

pub struct Databases {
    directory: PathBuf,
    loaded: BTreeMap<String, Arc<Dbc>>,
}

impl Databases {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            loaded: BTreeMap::new(),
        }
    }

    pub fn get(&mut self, name: &str, emit: &mut impl FnMut(&str)) -> Result<Arc<Dbc>, Error> {
        if let Some(database) = self.loaded.get(name) {
            return Ok(Arc::clone(database));
        }
        let path = self.directory.join(format!("{name}.dbc"));
        let text =
            std::fs::read_to_string(&path).map_err(|error| Error::DbcAsset { path, error })?;
        let database = parse(name, &text, emit)?;
        self.loaded.insert(name.to_owned(), Arc::clone(&database));
        Ok(database)
    }

    pub fn exists(&self, name: &str) -> bool {
        self.directory.join(format!("{name}.dbc")).exists()
    }
}
