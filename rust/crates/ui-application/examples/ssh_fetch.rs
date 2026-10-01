use openpilot_ui_application::{context::Translations, params::Read, services::ssh::Fetcher};
use openpilot_ui_framework::{callback::Callback, multilang::Multilang};
use serde::Deserialize;
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Scene {
    language: String,
    users: Vec<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, host, input, output] = args.as_slice() else {
        return Err("ssh_fetch ROOT HOST INPUT OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(input)?)?;
    let params = Arc::new(openpilot_params::Params::open(
        &Path::new(output).join("params"),
        "d",
    )?);
    let translations = Translations::new(Multilang::new(
        &Path::new(root).join("openpilot/selfdrive/ui/translations"),
        Some(&scene.language),
    )?);
    let mut fetcher = Fetcher::new(params.clone(), translations);
    fetcher.host = host.into();
    let output = Rc::new(RefCell::new(Vec::new()));
    for username in scene.users {
        params.put("GithubUsername", b"previous")?;
        params.put("GithubSshKeys", b"previous-keys")?;
        let count = output.borrow().len();
        let rows = output.clone();
        let params = params.clone();
        fetcher.fetch(username,Callback::new(move |error| {
            rows.borrow_mut().push(serde_json::json!({"error":error,"username":params.string("GithubUsername").unwrap_or_default(),"keys":params.string("GithubSshKeys").unwrap_or_default()}));
        }))?;
        let deadline = Instant::now() + Duration::from_secs(18);
        while output.borrow().len() == count {
            fetcher.update()?;
            if Instant::now() > deadline {
                return Err("SSH completion deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        fetcher.update()?;
    }
    println!("{}", serde_json::to_string(&*output.borrow())?);
    Ok(())
}
