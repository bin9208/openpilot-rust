use openpilot_carrot_server::{
    param_changes::{self, Change, History, Paths},
    param_restore::Restore,
    params::Backend,
    setting_profiles::{self, Creation, Git, ProfileError, ProfileStore},
    settings::Catalog,
    Error, Value,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

struct Session {
    backend: Backend,
    catalog: Catalog,
    history: History,
    state: PathBuf,
    git: Git,
}
impl Session {
    fn new(request: &Value) -> Result<Self, Error> {
        let root = PathBuf::from(request.get("root").string()?);
        let state = PathBuf::from(request.get("state").string()?);
        let backend = Backend::native(
            openpilot_params::Params::for_runtime_at(&root)?,
            state.clone(),
        );
        let history = History::new(Paths {
            log: state.join("param_changes.jsonl"),
            baseline: state.join("fingerprint_baseline.json"),
        })
        .with_timestamp(request.get("timestamp").clone());
        let git = Git {
            repository: PathBuf::from(request.get("repository").string()?),
            program: PathBuf::from(request.get("git_program").string()?),
        };
        Ok(Self {
            backend,
            catalog: Catalog::from_data(request.get("catalog").clone())?,
            history,
            state,
            git,
        })
    }
    fn apply(&mut self, request: &Value) -> Result<Value, ProfileError> {
        let catalog = self
            .catalog
            .with_gap_limits(self.backend.maximum_gap_levels())?;
        let path = self.state.join("setting_profiles.json");
        let store = ProfileStore::new(&path, &catalog);
        match request.get("action").string()?.as_str() {
            "read" => Ok(store.read()?),
            "write" => Ok(store.write(request.get("data"))?),
            "get" => Ok(store.get(request.get("id"))?.unwrap_or(Value::Null)),
            "snapshot" => Ok(store.snapshot(&self.backend)?),
            "create" => {
                let creation = Creation {
                    id: request.get("creation_id").string()?,
                    now: request.get("now").string()?,
                    meta: self.git.metadata(),
                };
                Ok(store.create(request.get("name"), &creation, &self.backend)?)
            }
            "update" => Ok(store.update(
                request.get("id"),
                request.get("data"),
                &request.get("now").string()?,
            )?),
            "delete" => {
                store.delete(request.get("id"))?;
                Ok(Value::Null)
            }
            "preview" => Ok(store.preview(
                request.get("id"),
                request.get("values"),
                &Restore::new(&mut self.backend, &catalog, &self.history),
            )?),
            "apply" => Ok(store.apply(
                request.get("id"),
                request.get("values"),
                &mut Restore::new(&mut self.backend, &catalog, &self.history),
            )?),
            "restore_preview" => Ok(Restore::new(&mut self.backend, &catalog, &self.history)
                .preview(request.get("values"), request.get("selected"))?),
            "restore_apply" => Ok(
                Restore::new(&mut self.backend, &catalog, &self.history).apply(
                    request.get("values"),
                    request.get("selected"),
                    request.get("source"),
                )?,
            ),
            "restore_raw" => Ok(Restore::new(&mut self.backend, &catalog, &self.history)
                .restore_values(request.get("values"), request.get("source"))?),
            "git_meta" => {
                let git = Git {
                    repository: self.git.repository.clone(),
                    program: if request.has("program") {
                        PathBuf::from(request.get("program").string()?)
                    } else {
                        self.git.program.clone()
                    },
                };
                Ok(git.metadata())
            }
            "unavailable_preview" => {
                let mut backend = Backend::memory(self.state.clone());
                Ok(Restore::new(&mut backend, &catalog, &self.history)
                    .preview(request.get("values"), &Value::Null)?)
            }
            "commit_url" => Ok(setting_profiles::commit_url(
                request.get("remote"),
                request.get("commit"),
            )?),
            "capture" => {
                let capture = Creation::capture(&self.git);
                Ok(Value::object([
                    ("id", Value::text(&capture.id)),
                    ("now", Value::text(&capture.now)),
                    ("meta", capture.meta),
                ]))
            }
            "append" => Ok(self
                .history
                .append(Change {
                    name: request.get("name"),
                    previous: request.get("prev"),
                    next: request.get("next"),
                    source: request.get("source"),
                    engaged: request.get("engaged").truth(),
                })
                .unwrap_or(Value::Null)),
            "history" => {
                use num_traits::ToPrimitive;
                let limit = request.get("limit").int()?.to_usize().unwrap_or(0);
                Ok(self
                    .history
                    .read(limit, request.get("name"), request.get("source"))?)
            }
            "verify" => Ok(self.history.verify()?),
            "observe" => Ok(Value::integer(self.history.observe(
                request.get("values"),
                if request.get("allowed").truth() {
                    Some(request.get("allowed"))
                } else {
                    None
                },
            )?)),
            "note" => {
                self.history
                    .note(request.get("name"), request.get("value"))?;
                Ok(Value::Null)
            }
            "fingerprint" => Ok(param_changes::fingerprint(request.get("values"))?),
            "read_baseline" => Ok(self.history.read_baseline()?.unwrap_or(Value::Null)),
            "write_baseline" => Ok(self.history.write_baseline(request.get("fingerprint"))?),
            "count_since" => Ok(Value::integer(self.history.count_since(
                request.get("timestamp"),
                if request.get("allowed").truth() {
                    Some(request.get("allowed"))
                } else {
                    None
                },
            )?)),
            "catalog" => {
                self.catalog = Catalog::from_data(request.get("data").clone())?;
                Ok(Value::Null)
            }
            "put" => {
                let name = request.get("name").string()?;
                let definition = self.catalog.by_name.get(&name);
                self.backend.put(
                    &name,
                    request.get("value"),
                    definition.truth().then_some(definition),
                )?;
                Ok(Value::Null)
            }
            _ => Err(Error::Source("unknown settings profile operation".into()).into()),
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = None;
    for line in io::stdin().lock().lines() {
        let request = Value::parse(&line?)?;
        let result = if request.get("action").text_eq("init") {
            session = Some(Session::new(&request)?);
            Ok(Value::Null)
        } else {
            session
                .as_mut()
                .ok_or_else(|| {
                    ProfileError::Service(Error::Source("session not initialized".into()))
                })?
                .apply(&request)
        };
        let output = match result {
            Ok(value) => value,
            Err(error) => {
                if let Some(code) = error.code() {
                    Value::object([
                        ("error", Value::text(&error.to_string())),
                        ("error_code", Value::text(code)),
                    ])
                } else {
                    Value::object([("error", Value::text(&error.to_string()))])
                }
            }
        };
        println!("{}", output.encode()?);
    }
    Ok(())
}
