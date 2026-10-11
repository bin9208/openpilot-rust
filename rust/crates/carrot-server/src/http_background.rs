use crate::{http::Application, Error};
use std::sync::Arc;
use tokio::{sync::watch, task::JoinHandle};

pub(crate) struct Background {
    heartbeat_stop: watch::Sender<bool>,
    heartbeat: Option<JoinHandle<Result<crate::heartbeat::LoopExit, Error>>>,
    git_stop: watch::Sender<bool>,
    git_status: Option<JoinHandle<Result<(), crate::git_status::Failure>>>,
    auto_stop: watch::Sender<bool>,
    auto_update: Option<JoinHandle<()>>,
    network_stop: watch::Sender<bool>,
    network: Option<JoinHandle<()>>,
    popular_upload: Option<JoinHandle<()>>,
    warm: JoinHandle<()>,
    precompress: JoinHandle<Result<(), Error>>,
}

impl Background {
    pub fn start(app: &Arc<Application>) -> Self {
        let (heartbeat_stop, heartbeat_stopped) = watch::channel(false);
        let heartbeat = app.heartbeat_params.clone().map(|params| {
            let service = Arc::clone(&app.heartbeat);
            let backend = crate::params::Backend::native(params, app.config.state.clone());
            tokio::spawn(async move { service.run_loop(backend, heartbeat_stopped).await })
        });
        let (git_stop, git_stopped) = watch::channel(false);
        let git_status = app
            .git_status
            .clone()
            .map(|service| tokio::spawn(async move { service.run_loop(git_stopped).await }));
        let (auto_stop, auto_stopped) = watch::channel(false);
        let auto_update = app.auto_update.clone().map(|service| {
            tokio::task::spawn_local(async move { service.run(auto_stopped).await })
        });
        let precompress = app.static_web.start_precompress();
        let (network_stop, network_stopped) = watch::channel(false);
        let network = app.network_refresh.then(|| {
            let target = Arc::clone(app);
            tokio::spawn(crate::system::background::run(target, network_stopped))
        });
        let popular_upload = app.popular_values.start_upload(Arc::clone(app));
        let warm_app = Arc::clone(app);
        let warm = tokio::task::spawn_blocking(move || {
            if let Err(error) = warm_app.settings_payload() {
                eprintln!("Settings warm cache: {error}");
            }
        });
        Self {
            heartbeat_stop,
            heartbeat,
            git_stop,
            git_status,
            auto_stop,
            auto_update,
            network_stop,
            network,
            popular_upload,
            warm,
            precompress,
        }
    }

    pub async fn finish(self, app: &Application) -> Result<(), Error> {
        self.warm.abort();
        self.network_stop.send_replace(true);
        if let Some(task) = self.network {
            if let Err(error) = task.await {
                eprintln!("device network cleanup: {error}");
            }
        }
        self.heartbeat_stop.send_replace(true);
        if let Some(task) = self.heartbeat {
            match task.await {
                Ok(Ok(
                    crate::heartbeat::LoopExit::Returned | crate::heartbeat::LoopExit::Cancelled,
                )) => {}
                Ok(Err(error)) => eprintln!("heartbeat cleanup: {error}"),
                Err(error) => eprintln!("heartbeat task: {error}"),
            }
        }
        self.git_stop.send_replace(true);
        if let Some(task) = self.git_status {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("Git status cleanup: {error}"),
                Err(error) => eprintln!("Git status task: {error}"),
            }
        }
        self.auto_stop.send_replace(true);
        if let Some(task) = self.auto_update {
            if let Err(error) = task.await {
                eprintln!("Auto update cleanup: {error}");
            }
        }
        if let Some(task) = self.popular_upload {
            task.abort();
            match task.await {
                Ok(()) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => eprintln!("popular startup refresh: {error}"),
            }
        }
        app.popular_values.shutdown().await?;
        if let Err(error) = self.warm.await {
            if !error.is_cancelled() {
                eprintln!("Settings warm task: {error}");
            }
        }
        self.precompress.abort();
        match self.precompress.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("static precompression: {error}"),
            Err(error) if error.is_cancelled() => {}
            Err(error) => eprintln!("static precompression task: {error}"),
        }
        Ok(())
    }
}
