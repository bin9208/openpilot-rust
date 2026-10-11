use crate::{http::Application, http_background::Background, Error};

struct Failures {
    first: Option<Error>,
}
impl Failures {
    fn record(&mut self, owner: &str, result: Result<(), Error>) {
        if let Err(error) = result {
            eprintln!("Application cleanup {owner}: {error}");
            self.first.get_or_insert(error);
        }
    }
    fn result(self) -> Result<(), Error> {
        match self.first {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

pub(crate) async fn finish(app: &Application, background: Background) -> Result<(), Error> {
    let mut failures = Failures { first: None };
    failures.record("dashcam sync", app.dashcam_sync_uploads.shutdown().await);
    if let Some(service) = &app.support_terminal {
        failures.record("support terminal", service.shutdown().await);
    }
    if let Some(service) = &app.terminal {
        failures.record("terminal", service.shutdown().await);
    }
    if let Some(service) = &app.live {
        failures.record("live", service.shutdown().await);
    }
    if let Some(service) = &app.web_navi {
        failures.record("navigation", service.shutdown().await);
    }
    if let Some(service) = &app.youtube_live {
        failures.record("YouTube", service.finish().await);
    }
    failures.record("background", background.finish(app).await);
    failures.record("Bluetooth", app.bluetooth_http.shutdown().await);
    if let Some(service) = &app.tools {
        failures.record("tools", service.shutdown().await);
    }
    failures.result()
}
