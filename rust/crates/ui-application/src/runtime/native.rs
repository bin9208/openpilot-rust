use super::*;
use crate::{
    context::{Actions, PrimeStatus, Translations},
    device::{Config as DeviceConfig, Device},
    params::store::Store,
    services,
    state::{ModelStatus, UiState},
};
use openpilot_ui_framework::{
    application::ApplicationConfig, multilang::Multilang, network::WifiSession,
};
use std::{cell::RefCell, path::Path, sync::Arc};
impl Runtime {
    pub fn native(root: &Path) -> Result<Self, crate::Error> {
        let device_type = openpilot_hardware_info::for_runtime()
            .get_device_type()
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        let mut platform = scheduling::Native { root: "/".into() };
        scheduling::bootstrap(&mut platform, device_type != "pc")?;
        let config = ApplicationConfig::for_runtime(root, "UI")?;
        let graphics = config.graphics;
        let language = config.language.clone();
        let app = Application::new(config)?;
        let params = Rc::new(Store::new(openpilot_params::Params::for_runtime()?));
        let memory = Rc::new(Store::new(openpilot_params::Params::for_runtime_at(
            Path::new("/dev/shm/params"),
        )?));
        let persist_root = PathBuf::from(
            openpilot_hardware_info::paths::Paths::default()
                .persist_root()
                .map_err(|error| Error::Io(std::io::Error::other(error)))?,
        );
        let cache = std::env::var_os("CARROT_BIG_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if Path::new("/TICI").is_file() {
                    "/data/media/0/carrot/models".into()
                } else {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                        .join(".comma/models")
                }
            });
        let paths = || openpilot_usbgpu::model::Paths {
            models: root.join("openpilot/selfdrive/modeld/models"),
            cache: cache.clone(),
        };
        let models = Arc::new(paths());
        let model_status = Rc::new(move || {
            openpilot_usbgpu::model::status(&models)
                .map(|status| ModelStatus {
                    compiled: status.compiled,
                    compile_pending: status.compile_pending,
                })
                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))
        });
        let now = Rc::new(openpilot_startup_ui::diagnostics::monotonic_now);
        let prime =
            services::prime::initial(params.as_ref(), std::env::var("PRIME_TYPE").ok().as_deref())?;
        let context = Context {
            ui: Rc::new(RefCell::new(UiState::new(
                params.as_ref(),
                now(),
                model_status()?,
            )?)),
            messages: Rc::new(RefCell::new(
                openpilot_messaging::runtime::SubMaster::for_runtime(
                    &messages::SERVICES,
                    Default::default(),
                )
                .map_err(|error| Error::Io(std::io::Error::other(error)))?,
            )),
            device: Rc::new(RefCell::new(Device::new(DeviceConfig {
                big: graphics.big,
                pc: graphics.pc,
                mici: device_type == "mici",
                target_fps: 20,
            }))),
            params,
            memory,
            translations: Translations::new(
                Multilang::new(
                    &root.join("openpilot/selfdrive/ui/translations"),
                    Some(&language),
                )
                .map_err(|error| Error::Io(std::io::Error::other(error)))?,
            ),
            prime: Arc::new(PrimeStatus::new(prime)),
            api: services::Api::new(
                std::env::var("API_HOST").unwrap_or_else(|_| "https://api.commadotai.com".into()),
                root.into(),
                persist_root.clone(),
            ),
            poll_gate: Arc::default(),
            actions: Actions::default(),
            big: graphics.big,
            pc: graphics.pc,
            device_type: device_type.clone(),
            now_monotonic: now,
            model_status,
            now_wall: Rc::new(chrono::Local::now),
            source_root: root.into(),
            persist_root,
            callbacks: Rc::default(),
        };
        context.sync_services();
        let launcher = std::env::current_exe()?.with_file_name("openpilot-process-child");
        let manager = openpilot_wifi::WifiManager::start(openpilot_wifi::Config {
            launcher: launcher.clone(),
            ..Default::default()
        })
        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        let resources = Resources {
            network: crate::settings::resources::Network {
                session: WifiSession::new(manager)?,
                params: Rc::new(context.params.raw.as_ref().clone()),
            },
            ticks: app.ticks.clone(),
            egpu: Arc::new(services::egpu::native::Native {
                options: openpilot_usbgpu::check::Options::for_runtime()
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?,
                models: paths(),
            }),
        };
        let hardware = Box::new(hardware::Native::new(&device_type, launcher)?);
        Ok(Self::new(
            app,
            context,
            resources,
            hardware,
            "camerad",
            std::env::current_exe()?.with_file_name("openpilot-updated"),
        )?)
    }
}
