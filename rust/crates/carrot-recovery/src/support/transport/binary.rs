use super::*;
use std::{
    fs::{File, Permissions},
    os::unix::fs::PermissionsExt,
};

fn executable(path: &std::path::Path) -> bool {
    path.is_file()
        && rustix::fs::accessat(
            rustix::fs::CWD,
            path,
            rustix::fs::Access::EXEC_OK,
            rustix::fs::AtFlags::empty(),
        )
        .is_ok()
}

pub(super) fn resolve(config: &Config) -> Result<PathBuf, Error> {
    let configured = std::env::var("CARROT_CLOUDFLARED_BIN").unwrap_or_default();
    let configured = helpers::strip(&configured);
    if !configured.is_empty() {
        return Ok(configured.into());
    }
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join("cloudflared"))
            .find(|path| executable(path))
    }) {
        return Ok(path);
    }
    let local = config.data.join("bin/cloudflared");
    if executable(&local) {
        return Ok(local);
    }
    let system = rustix::system::uname();
    let platform = system.sysname().to_string_lossy();
    let machine = system.machine().to_string_lossy();
    if platform.to_lowercase() != "linux" {
        return Err(Error::Source(format!(
            "cloudflared unavailable on {platform} {machine}"
        )));
    }
    let asset = match machine.to_lowercase().as_str() {
        "x86_64" | "amd64" => "amd64",
        "aarch64" | "arm64" => "arm64",
        "armv7l" => "arm",
        _ => {
            return Err(Error::Source(format!(
                "cloudflared auto-install unsupported on {machine}"
            )))
        }
    };
    let url=format!("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-{asset}");
    download(config, &url)
}

pub(super) fn download(config: &Config, url: &str) -> Result<PathBuf, Error> {
    let local = config.data.join("bin/cloudflared");
    let parent = local
        .parent()
        .ok_or_else(|| Error::Source("cloudflared parent missing".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = local.with_file_name("cloudflared.tmp");
    let timeout = Duration::from_secs(60);
    let roots: ureq::tls::RootCerts = rustls_native_certs::load_native_certs()
        .certs
        .iter()
        .map(|cert| ureq::tls::Certificate::from_der(cert.as_ref()).to_owned())
        .collect::<Vec<_>>()
        .into();
    let settings = ureq::Agent::config_builder()
        .timeout_global(None)
        .timeout_connect(Some(timeout))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(roots).build())
        .build();
    let agent = openpilot_http_transport::socket_timeout_agent(settings, timeout);
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| Error::Source(error.to_string()))?;
    let mut file = File::create(&temporary)?;
    std::io::copy(&mut response.body_mut().as_raw_reader(), &mut file)?;
    drop(file);
    fs::set_permissions(&temporary, Permissions::from_mode(0o755))?;
    fs::rename(&temporary, &local)?;
    Ok(local)
}
