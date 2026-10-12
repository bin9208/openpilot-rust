use crate::Error;
use std::{fs::Permissions, os::unix::fs::PermissionsExt, path::PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct Config {
    pub local: PathBuf,
    pub download: String,
    pub platform: String,
}
impl Config {
    pub fn original(data: &std::path::Path) -> Self {
        let machine = match std::env::consts::ARCH {
            "aarch64" => "aarch64",
            "arm" => "armv7l",
            other => other,
        };
        let asset = match machine {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            "armv7l" => "arm",
            _ => "",
        };
        Self {
            local: data.join("bin/cloudflared"),
            download: if cfg!(target_os = "linux") && !asset.is_empty() {
                format!("https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-{asset}")
            } else {
                String::new()
            },
            platform: format!(
                "{} {machine}",
                if cfg!(target_os = "linux") {
                    "Linux"
                } else {
                    std::env::consts::OS
                }
            ),
        }
    }
    pub fn path(&self) -> Option<PathBuf> {
        let configured = std::env::var("CARROT_CLOUDFLARED_BIN").unwrap_or_default();
        let configured = configured.trim_matches(crate::tools::text::whitespace);
        if !configured.is_empty() {
            return Some(configured.into());
        }
        let executable = |path: &std::path::Path| {
            path.is_file()
                && rustix::fs::accessat(
                    rustix::fs::CWD,
                    path,
                    rustix::fs::Access::EXEC_OK,
                    rustix::fs::AtFlags::empty(),
                )
                .is_ok()
        };
        if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|directory| directory.join("cloudflared"))
                .find(|path| executable(path))
        }) {
            return Some(path);
        }
        executable(&self.local).then(|| self.local.clone())
    }
    pub fn automatic(&self) -> bool {
        !matches!(
            std::env::var("CARROT_SUPPORT_AUTO_INSTALL_CLOUDFLARED")
                .unwrap_or_else(|_| "1".into())
                .trim()
                .to_lowercase()
                .as_str(),
            "0" | "false" | "no" | "off"
        )
    }
    pub async fn resolve(&self, client: &reqwest::Client) -> Result<PathBuf, Error> {
        if let Some(path) = self.path() {
            return Ok(path);
        }
        if !self.automatic() {
            return Err(Error::Source("cloudflared not found".into()));
        }
        if self.download.is_empty() {
            return Err(Error::Source(format!(
                "cloudflared not found and auto-install is unsupported on {}",
                self.platform
            )));
        }
        let parent = self
            .local
            .parent()
            .ok_or_else(|| Error::Source("cloudflared parent missing".into()))?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = self.local.with_file_name("cloudflared.tmp");
        let transfer = async {
            let mut response = client
                .get(&self.download)
                .send()
                .await
                .map_err(|error| Error::Source(error.to_string()))?
                .error_for_status()
                .map_err(|error| Error::Source(error.to_string()))?;
            let mut file = tokio::fs::File::create(&temporary).await?;
            while let Some(bytes) = response
                .chunk()
                .await
                .map_err(|error| Error::Source(error.to_string()))?
            {
                file.write_all(&bytes).await?;
            }
            drop(file);
            tokio::fs::set_permissions(&temporary, Permissions::from_mode(0o755)).await?;
            tokio::fs::rename(&temporary, &self.local).await?;
            Ok::<_, Error>(self.local.clone())
        }
        .await;
        match transfer {
            Ok(path) => Ok(path),
            Err(error) => {
                match tokio::fs::remove_file(temporary).await {
                    Ok(()) => {}
                    Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => {}
                    Err(cleanup) => eprintln!("Support download cleanup: {cleanup}"),
                }
                Err(Error::Source(format!(
                    "cloudflared download failed: {error}"
                )))
            }
        }
    }
}
