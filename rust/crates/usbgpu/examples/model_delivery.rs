use openpilot_usbgpu::{
    model::Manifest,
    model_delivery::{self, DownloadKind, Error},
};
use serde::Deserialize;
use std::{io, path::PathBuf, time::Duration};

#[derive(Deserialize)]
struct Input {
    action: String,
    cache: PathBuf,
    model: Manifest,
    ca: Option<PathBuf>,
    #[serde(default)]
    value: serde_json::Value,
}

#[derive(Deserialize)]
struct StatusTrace {
    initial_wall: f64,
    events: Vec<StatusEvent>,
}
#[derive(Deserialize)]
struct StatusEvent {
    phase: model_delivery::status::Phase,
    wall: f64,
    monotonic: f64,
    force: bool,
    downloaded: u64,
}

#[derive(Deserialize)]
struct FailureInput {
    detail: String,
    kind: model_delivery::failure::Kind,
    phase: String,
    wall_time: f64,
}

fn agent(ca: Option<&std::path::Path>, seconds: u64) -> Result<ureq::Agent, Error> {
    Ok(if let Some(ca) = ca {
        let cert = ureq::tls::Certificate::from_pem(&std::fs::read(ca)?)?;
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::Specific(vec![cert].into()))
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .timeout_global(None)
            .build();
        openpilot_http_transport::socket_timeout_agent(config, Duration::from_secs(seconds))
    } else {
        model_delivery::agent(seconds)
    })
}

fn run() -> Result<(), Error> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let agent = agent(input.ca.as_deref(), 30)?;
    let mut progress = Vec::new();
    let mut observe = |done, total| progress.push([done, total]);
    let result = match input.action.as_str() {
        "ensure" => model_delivery::ensure(&agent, &input.model, &input.cache, &mut observe)
            .map(|(_, changed)| serde_json::json!({"changed":changed})),
        "download" | "precompiled" => {
            let kind = if input.action == "download" {
                DownloadKind::Model
            } else {
                DownloadKind::Precompiled
            };
            model_delivery::download(
                &agent,
                &input.model.url,
                &input.cache.join(input.model.cache_filename()),
                input.model.size,
                &input.model.sha256,
                kind,
                &mut observe,
            )
            .map(|_| serde_json::json!({"ok":true}))
        }
        "manifest" => model_delivery::fetch_manifest(&agent, &input.model.url)
            .map(|model| serde_json::json!({"manifest":model})),
        "inspect" => model_delivery::parse_manifest(
            &serde_json::to_vec(&input.value)?,
            model_delivery::DEFAULT_MANIFEST_URL,
        )
        .map(|model| serde_json::json!({"cache_filename":model.cache_filename(),"manifest":model})),
        "state" => model_delivery::State::read(&input.cache)
            .map(|state| serde_json::json!({"state":state})),
        "catalog" => model_delivery::catalog::Catalog::parse(
            &serde_json::to_vec(&input.value["catalog"])?,
            &input.model.sha256,
            input.value["url"].as_str().unwrap_or(""),
        )
        .map(|catalog| serde_json::json!({"catalog":catalog.value})),
        "catalog-model" => {
            let model = model_delivery::parse_manifest(
                &serde_json::to_vec(&input.value["model"])?,
                model_delivery::DEFAULT_MANIFEST_URL,
            )?;
            let url = model_delivery::precompiled::catalog_url(&model)?;
            model_delivery::catalog::Catalog::parse(
                &serde_json::to_vec(&input.value["catalog"])?,
                &model.sha256,
                &url,
            )
            .map(|catalog| serde_json::json!({"model":model,"url":url,"catalog":catalog.value}))
        }
        "assets" => model_delivery::assets::validate(
            std::path::Path::new(input.value.as_str().unwrap_or("")),
            &input.model.sha256,
            input.model.size,
        )
        .map(|package| serde_json::json!({"checkpoint":package.checkpoint})),
        "ready" => {
            let paths = openpilot_usbgpu::model::Paths {
                cache: input.cache,
                models: PathBuf::from(input.value["models"].as_str().unwrap_or("")),
                assets: PathBuf::from(input.value["assets"].as_str().unwrap_or("")),
            };
            let status = openpilot_usbgpu::model::status(&paths)?;
            Ok(
                serde_json::json!({"compiled":status.compiled,"compile_pending":status.compile_pending,
                "path":openpilot_usbgpu::model::active_compiled_path(&paths)}),
            )
        }
        "failure" => {
            let failure: FailureInput = serde_json::from_value(input.value)?;
            model_delivery::failure::record(
                &input.cache.join("model.pkl"),
                &failure.detail,
                failure.kind,
                &failure.phase,
                failure.wall_time,
            )
            .map(|rejected| serde_json::json!({"rejected":rejected}))
            .map_err(Error::from)
        }
        "status" => {
            let trace: StatusTrace = serde_json::from_value(input.value)?;
            let mut reporter =
                model_delivery::status::Reporter::new(&input.cache, trace.initial_wall);
            let mut rows = Vec::new();
            for event in trace.events {
                rows.push(
                    reporter.update(
                        event.phase,
                        model_delivery::status::Values {
                            model: Some(&input.model),
                            downloaded: Some(event.downloaded),
                            detail: None,
                        },
                        event.force,
                        Duration::try_from_secs_f64(event.monotonic)
                            .map_err(|error| Error::Invalid(error.to_string()))?,
                        event.wall,
                    )?,
                );
            }
            Ok(serde_json::json!({"rows":rows,"last":model_delivery::status::read(&input.cache)}))
        }
        "archive" => model_delivery::archive::install(
            std::path::Path::new(input.value.as_str().unwrap_or("")),
            &input.cache,
        )
        .map(|()| serde_json::json!({"installed":true})),
        "install" => model_delivery::precompiled::ensure(
            &self::agent(input.ca.as_deref(), 8)?,
            &agent,
            &input.model,
            &input.cache,
            std::path::Path::new(input.value.as_str().unwrap_or("")),
            &mut observe,
        )
        .map(|path| serde_json::json!({"installed":path.is_some()})),
        "observed" => {
            let mut events = Vec::new();
            model_delivery::download_observed(
                &agent,
                &input.model.url,
                &input.cache.join(input.model.cache_filename()),
                input.model.size,
                &input.model.sha256,
                DownloadKind::Model,
                &mut |event| {
                    events.push(match event {
                        model_delivery::Event::Progress { downloaded, total } => {
                            serde_json::json!(["progress", downloaded, total])
                        }
                        model_delivery::Event::Verifying => serde_json::json!(["verifying"]),
                    });
                },
            )
            .map(|_| serde_json::json!({"events":events}))
        }
        _ => return Err(Error::Invalid("unknown delivery action".into())),
    };
    let value = match result {
        Ok(value) => serde_json::json!({"result":value,"progress":progress}),
        Err(error) => serde_json::json!({"error":error.to_string(),"progress":progress}),
    };
    serde_json::to_writer(io::stdout().lock(), &value)?;
    Ok(())
}
fn main() -> Result<(), Error> {
    run()
}
