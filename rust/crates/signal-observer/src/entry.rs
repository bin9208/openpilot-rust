//! Native producer selection; manager eligibility remains independently gated.
use crate::{
    auto_runtime,
    backend::{self, Backend, Request},
    runtime::{self, Config, Error},
};
use openpilot_params::Params;
use openpilot_plannerd::signal::transport::Mode;
use serde_json::Value;
use std::{fs, path::Path, process::Command, sync::atomic::AtomicBool};
/// Dispatch prefix only; model/tracker acquisition remains in the selected owner.
#[derive(Debug)]
pub enum Route {
    Classical,
    Disabled,
    Neural(Request),
}
pub fn select(
    mode: &mut impl FnMut() -> Mode,
    neural: &mut impl FnMut() -> Result<bool, Error>,
    requested: &mut impl FnMut() -> Result<bool, Error>,
    backend: &mut impl FnMut() -> Result<Request, Error>,
) -> Result<Route, Error> {
    if mode() == Mode::Prepare {
        return Ok(Route::Classical);
    }
    if !neural()? {
        return Ok(Route::Classical);
    }
    if !requested()? || !neural()? {
        return Ok(Route::Disabled);
    }
    Ok(Route::Neural(backend()?))
}
pub struct Entry<'a> {
    pub config: &'a Config,
    pub stop: &'a AtomicBool,
    pub params: &'a Params,
    pub provider: Option<&'a Path>,
    pub child: Option<Backend>,
}
impl Entry<'_> {
    pub fn run(
        &self,
        mode: &mut impl FnMut() -> Mode,
        event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
        spawn: &mut impl FnMut(Backend) -> Result<Command, Error>,
    ) -> Result<(), Error> {
        fs::create_dir_all(&self.config.directory)?;
        if let Some(child) = self.child {
            return match child {
                Backend::TrackingCpu => {
                    runtime::run_classical_comparison(self.config, self.stop, mode, event)
                }
                Backend::Qcom => self.qcom(mode, event),
                Backend::Cpu => {
                    Err("automatic signal child cannot select CPU neural fallback".into())
                }
            };
        }
        // Separate prefix decisions from resource-owning tracker/model entry.
        // The mode callback is shared sequentially by the prefix predicates.
        let initial_mode = mode();
        let route = select(
            &mut || initial_mode,
            &mut || auto_runtime::flag(self.config, "neural_enabled"),
            &mut || {
                if self.config.directory == Path::new(runtime::DIRECTORY) {
                    Ok(mode().enabled())
                } else {
                    auto_runtime::flag(self.config, "enabled")
                }
            },
            &mut || auto_runtime::selection(self.config),
        )?;
        let selection = match route {
            Route::Classical => {
                return runtime::run_classical_comparison(self.config, self.stop, mode, event);
            }
            Route::Disabled => return Ok(()),
            Route::Neural(selection) => selection,
        };
        match selection {
            Request::Cpu => self.cpu(mode, event),
            Request::Auto | Request::Qcom => auto_runtime::run(
                self.config,
                self.stop,
                mode,
                &mut || {
                    // Source read_mode catches errors; boolean read failures
                    // retain the native Params error and stop the old child.
                    let raw = self
                        .params
                        .get("SignalAssistMode")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    backend::automatic_read(
                        Mode::from_parameter(runtime::parameter_value(&raw)),
                        |key| {
                            self.params
                                .get_bool(key)
                                .map_err(|error| Box::new(error) as Error)
                        },
                    )
                },
                spawn,
                event,
            ),
        }
    }
    #[cfg(feature = "neural-ort")]
    fn cpu(
        &self,
        mode: &mut impl FnMut() -> Mode,
        event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    ) -> Result<(), Error> {
        crate::neural_runtime::run_cpu_selected(
            self.config,
            self.provider
                .ok_or("native signal ORT provider root is not configured")?,
            self.stop,
            mode,
            event,
        )
    }
    #[cfg(not(feature = "neural-ort"))]
    fn cpu(
        &self,
        _mode: &mut impl FnMut() -> Mode,
        _event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    ) -> Result<(), Error> {
        Err("native signal CPU inference requires the verified neural-ort feature".into())
    }
    fn qcom(
        &self,
        mode: &mut impl FnMut() -> Mode,
        event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let Some(root) = std::env::var_os("CARROT_RUNTIME_ROOT") {
            crate::qcom::require_native_cached(Path::new(&root))?;
        } else {
            crate::qcom::require_cached(&mut |model, suffix| {
                crate::qcom::cached_file(
                    &self
                        .config
                        .directory
                        .join("neural")
                        .join(format!("{model}.qcom.{suffix}")),
                )
            })?;
        }
        #[cfg(feature = "neural-qcom")]
        {
            crate::neural_runtime::run_qcom_selected(self.config, self.stop, mode, event)
        }
        #[cfg(not(feature = "neural-qcom"))]
        {
            let _ = (mode, event);
            Err("native signal QCOM provider was not built".into())
        }
    }
}
