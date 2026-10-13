use crate::{
    processes::{Entry, Processes},
    Error,
};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_manager_catalog::{
    binding::{self, Binding, Installation},
    catalog, ImportConfig,
};
use openpilot_process_supervision::{
    Execution, ManagedProcess, NativeCommand, ParamsSource, PersistentCommand,
    PersistentDaemonProcess, ProcessLog,
};

pub fn processes(installation: &Installation, factory: &Factory) -> Result<Processes, Error> {
    let config = ImportConfig::from_environment(
        installation
            .root
            .join("openpilot/tools/bodyteleop/web.py")
            .is_file(),
    );
    let mut entries = Vec::new();
    for descriptor in catalog(config) {
        let binding = binding::resolve(descriptor, installation);
        let process = match binding {
            Some(binding) => match binding.validate() {
                Ok(()) => Some(ManagedProcess::new(
                    descriptor.name.into(),
                    execution(binding, installation),
                    ProcessLog::new(factory.logger()),
                )),
                Err(error) => {
                    factory.logger().emit(
                        log_site!(),
                        Record::text(
                            Level::Warning,
                            format!("native catalog {} unavailable: {error}", descriptor.name),
                        ),
                    )?;
                    None
                }
            },
            None => None,
        };
        entries.push(Entry {
            descriptor,
            process,
        });
    }
    Ok(Processes { entries })
}

fn execution(binding: Binding, installation: &Installation) -> Execution {
    let launcher = installation.binaries.join("openpilot-process-child");
    match binding.pid_key {
        Some(key) => Execution::Persistent(PersistentDaemonProcess::new(
            PersistentCommand {
                launcher,
                identity: binding.argv[0].to_string_lossy().into_owned(),
                argv: binding.argv,
                pid_param: key.into(),
            },
            ParamsSource::Runtime,
        )),
        None => Execution::Native(NativeCommand {
            launcher,
            basedir: installation.root.clone(),
            cwd: binding.cwd,
            argv: binding.argv,
        }),
    }
}

pub fn inspect(installation: &Installation) -> Vec<serde_json::Value> {
    catalog(ImportConfig::from_environment(installation.root.join("openpilot/tools/bodyteleop/web.py").is_file())).into_iter().map(|entry| {
        match binding::resolve(entry, installation) {
            Some(bound) => {
                let error = bound.validate().err().map(|error| error.to_string());
                let requirements: Vec<_> = bound.requirements.iter().map(|required| match required {
                    binding::Requirement::Executable(path) => serde_json::json!({"path":path,"executable":true}),
                    binding::Requirement::Directory(path) => serde_json::json!({"path":path,"directory":true}),
                    binding::Requirement::File(path) => serde_json::json!({"path":path,"file":true}),
                }).collect();
                serde_json::json!({"name":entry.name,"enabled":entry.enabled,"argv":bound.argv,"cwd":bound.cwd,
                    "pid_key":bound.pid_key,"requirements":requirements,"ready":error.is_none(),"error":error})
            },
            None => serde_json::json!({"name":entry.name,"enabled":entry.enabled,"ready":false,"error":"native implementation unavailable"}),
        }
    }).collect()
}
