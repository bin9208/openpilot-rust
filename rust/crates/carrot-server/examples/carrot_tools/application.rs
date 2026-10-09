use openpilot_carrot_server::{config::Config, http::Application, params::Backend, tools};
use std::{path::PathBuf, sync::Arc};

pub struct Fixture {
    pub root: PathBuf,
    pub config: tools::config::Config,
}
pub async fn run(
    fixture: Fixture,
    listener: tokio::net::TcpListener,
    stopped: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = fixture.root;
    let mut config = Config::at(
        &std::env::current_dir()?,
        &root,
        &root.join("settings.json"),
    );
    config.web = root.join("web");
    config.legacy_state = root.join("legacy-state");
    config.params_backup = fixture.config.paths.backup.clone();
    // Instantiate unrelated feature services without Params; this fixture never
    // starts external recipients. Tools and the already-proved Params routes
    // receive the caller-owned actual Params namespace before serving.
    let mut app = Application::new(config, Backend::memory(root.join("state")));
    let writable = Arc::get_mut(&mut app).ok_or("fixture Application already shared")?;
    if let Some(params) = fixture.config.params.clone() {
        *writable
            .params
            .get_mut()
            .map_err(|_| "fixture Params poisoned")? = Backend::native(params, root.join("state"));
    }
    let tools = tools::service::Service::new(fixture.config);
    writable.tools = Some(Arc::clone(&tools));
    openpilot_carrot_server::http::serve(app, listener, async {
        let _stopped = stopped.await;
    })
    .await?;
    std::fs::write(
        root.join("app-cleanup.json"),
        tools.jobs.snapshots(20)?.encode()?,
    )?;
    Ok(())
}
