use super::Error;
use crate::{api::http::Session, context::Translations};
use openpilot_params::Params;
use openpilot_ui_framework::callback::Callback;
use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Default)]
struct Completion {
    done: bool,
    error: Option<String>,
}
pub struct Fetcher {
    params: Arc<Params>,
    translations: Translations,
    completion: Arc<Mutex<Completion>>,
    callback: Option<Callback<Option<String>>>,
    workers: Vec<JoinHandle<()>>,
    pub host: String,
}
impl Fetcher {
    pub fn new(params: Arc<Params>, translations: Translations) -> Self {
        Self {
            params,
            translations,
            completion: Arc::default(),
            callback: None,
            workers: Vec::new(),
            host: "https://github.com".into(),
        }
    }
    pub fn fetch(
        &mut self,
        username: String,
        callback: Callback<Option<String>>,
    ) -> Result<(), std::io::Error> {
        self.completion
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .error = None;
        self.callback = Some(callback);
        let params = self.params.clone();
        let translations = self.translations.clone();
        let completion = self.completion.clone();
        let host = self.host.clone();
        self.workers.push(
            thread::Builder::new()
                .name("ui-ssh-keys".into())
                .spawn(move || {
                    let mut session = Session::new(
                        "python-requests/2.34.2".into(),
                        Some(Duration::from_secs(15)),
                    );
                    let result = session.get(&format!("{host}/{username}.keys"), None);
                    let result = match result {
                        Ok(response)
                            if response.status < 400 && !strip(&response.text).is_empty() =>
                        {
                            params
                                .put("GithubUsername", username.as_bytes())
                                .and_then(|()| {
                                    params.put("GithubSshKeys", strip(&response.text).as_bytes())
                                })
                                .map_err(|_| false)
                        }
                        Err(error) => Err(error.timed_out()),
                        _ => Err(false),
                    };
                    let mut completion =
                        completion.lock().unwrap_or_else(|error| error.into_inner());
                    if let Err(timeout) = result {
                        completion.error = Some(if timeout {
                            translations.tr("Request timed out")
                        } else {
                            translations
                                .tr("No SSH keys found for user '{}'")
                                .replace("{}", &username)
                        });
                    }
                    completion.done = true;
                })?,
        );
        Ok(())
    }
    pub fn clear(&self) -> Result<(), Error> {
        for key in ["GithubUsername", "GithubSshKeys"] {
            match self.params.remove(key) {
                Ok(()) => {}
                Err(openpilot_params::Error::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn update(&mut self) -> Result<(), Error> {
        let mut index = 0;
        while index < self.workers.len() {
            if self.workers[index].is_finished() {
                let worker = self.workers.swap_remove(index);
                if worker.join().is_err() {
                    return Err(Error::Contract("SSH worker panicked"));
                }
            } else {
                index += 1;
            }
        }

        let error = {
            let mut completion = self
                .completion
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if !completion.done {
                return Ok(());
            }
            completion.done = false;
            completion.error.clone()
        };
        if error.is_some() {
            self.clear()?;
        }
        if let Some(callback) = &self.callback {
            callback.call(error);
        }
        Ok(())
    }
}

fn strip(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
}
