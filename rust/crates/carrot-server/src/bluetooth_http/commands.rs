use super::{bad, Failure, Service};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

impl Service {
    pub(super) async fn radio_enabled(&self) -> Result<bool, Failure> {
        let mut child = Command::new(&self.command)
            .args(["-n", "test", "-f", "/data/bluetooth/ENABLED"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
            Ok(status) => Ok(status?.success()),
            Err(_) => {
                child.kill().await?;
                child.wait().await?;
                Ok(false)
            }
        }
    }

    pub(super) async fn radio(&self, enabled: bool) -> Result<(), Failure> {
        let commands = if enabled {
            [
                ["mkdir", "-p", "/data/bluetooth"],
                ["touch", "/data/bluetooth/ENABLED", ""],
                ["systemctl", "start", "carrot-bluetooth-radio"],
            ]
        } else {
            [
                ["rm", "-f", "/data/bluetooth/ENABLED"],
                ["systemctl", "stop", "carrot-bluetooth-radio"],
                ["systemctl", "stop", "bluetooth"],
            ]
        };
        for args in commands {
            let mut child = Command::new(&self.command)
                .arg("-n")
                .args(args.into_iter().filter(|arg| !arg.is_empty()))
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            let mut pipe = child
                .stderr
                .take()
                .ok_or_else(|| bad("radio stderr unavailable"))?;
            let mut reader = tokio::spawn(async move {
                let mut bytes = Vec::new();
                pipe.read_to_end(&mut bytes).await?;
                Ok::<_, std::io::Error>(bytes)
            });
            let completed = tokio::time::timeout(Duration::from_secs(20), async {
                let status = child.wait().await?;
                let bytes = (&mut reader).await.map_err(std::io::Error::other)??;
                Ok::<_, std::io::Error>((status, bytes))
            })
            .await;
            match completed {
                Err(_) => {
                    child.kill().await?;
                    child.wait().await?;
                    reader.abort();
                    return Err(bad("radio operation timed out"));
                }
                Ok(Err(error)) => {
                    reader.abort();
                    return Err(error.into());
                }
                Ok(Ok((status, bytes))) if !status.success() => {
                    return Err(bad(&String::from_utf8_lossy(&bytes)
                        .chars()
                        .take(500)
                        .collect::<String>()))
                }
                Ok(Ok(_)) => {}
            }
        }
        Ok(())
    }
}
