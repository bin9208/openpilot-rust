use super::{attachment, parameters, Upload};
use base64::Engine;
use openpilot_web_upload::Response;
use std::fs;

const OBFUSCATED: &str = concat!(
    "CxUGAhxOAkocChYTGxsLQE4ZXEwAAhtAA0gHEAwKGwdGXlsfQgdTUEVKXkEcUkpaVEdEWkAkRwMCFzEL",
    "MF8zS1YzWh8YJlkpVk4EUwQ0ED02IkgXKjMkQzIYIRt/HgUWUTUQWCcaAS1XKhpFUT4cGDBnLiACOx1DXQ=="
);
fn webhook(upload: &Upload<'_>) -> String {
    if std::env::var("CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE")
        .is_ok_and(|v| ["1", "true", "yes", "on"].contains(&v.trim().to_lowercase().as_str()))
    {
        return String::new();
    }
    for name in [
        "CARROT_EXCEPTION_DISCORD_WEBHOOK_URL",
        "CARROT_DISCORD_WEBHOOK_URL",
        "DISCORD_WEBHOOK_URL",
    ] {
        if let Ok(value) = std::env::var(name) {
            if !value.trim().is_empty() {
                return value.trim().into();
            }
        }
    }
    for name in [
        "CarrotExceptionDiscordWebhookUrl",
        "CarrotDiscordWebhookUrl",
        "CarrotDiscordWebhookURL",
        "DiscordWebhookUrl",
        "DiscordWebhookURL",
    ] {
        let value = parameters::text(upload.params, name);
        if !value.is_empty() {
            return value;
        }
    }
    let Ok(mut bytes) = base64::engine::general_purpose::STANDARD.decode(OBFUSCATED) else {
        return String::new();
    };
    let key = b"carrot-exception-v1";
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte ^= key[index % key.len()];
    }
    String::from_utf8(bytes)
        .map(|s| s.trim().into())
        .unwrap_or_default()
}
fn text(upload: &Upload<'_>, key: &str, default: &str) -> String {
    let value = parameters::text(upload.params, key);
    if value.is_empty() {
        default.into()
    } else {
        value
    }
}
fn repo(upload: &Upload<'_>) -> String {
    let remote = text(upload, "GitRemote", "");
    let remote = remote.strip_prefix("git@github.com:").map_or_else(
        || remote.clone(),
        |path| format!("https://github.com/{path}"),
    );
    if remote.starts_with("https://github.com/") || remote.starts_with("http://github.com/") {
        return remote.trim_end_matches(".git").replacen(
            "http://github.com/",
            "https://github.com/",
            1,
        );
    }
    let username = text(upload, "GithubUsername", "");
    if username.is_empty() {
        "https://github.com/ajouatom/openpilot".into()
    } else {
        format!("https://github.com/{username}/openpilot")
    }
}
fn content(upload: &Upload<'_>, why: &str, web_ok: bool, response: Option<&Response>) -> String {
    let repository = repo(upload);
    let commit = text(upload, "GitCommit", "unknown");
    let commit_text = if commit == "unknown" {
        "unknown".into()
    } else {
        format!(
            "[{}]({repository}/commit/{commit})",
            commit.chars().take(8).collect::<String>()
        )
    };
    let status = response.map_or(String::new(), |r| format!(" ({})", r.status));
    format!("# Carrot Exception\n### Upload\n- Time: {}\n- Reason: {why}\n- Web: {}{status}\n### Device\n- Car name: {}\n- DongleId: {}\n- Serial: {}\n- GitHub: {repository}\n- Branch: {}\n- Commit: {commit_text} ({})",chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),if web_ok{"ok"}else{"failed"},text(upload,"CarName","none"),text(upload,"DongleId","unknown"),text(upload,"HardwareSerial","unknown"),text(upload,"GitBranch","unknown"),text(upload,"GitCommitDate","unknown")).chars().take(1900).collect()
}
fn escape(value: &str) -> String {
    value
        .replace('\n', "%0A")
        .replace('\r', "%0D")
        .replace('"', "%22")
}
fn file(body: &mut Vec<u8>, boundary: &str, field: &str, name: &str, mime: &str, bytes: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{}\"\r\nContent-Type: {mime}\r\n\r\n",escape(name)).as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}
pub fn send(
    upload: &Upload<'_>,
    why: &str,
    web_ok: bool,
    response: Option<&Response>,
    settings: bool,
) -> bool {
    let url = webhook(upload);
    if url.is_empty() || !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }
    let payload = serde_json::json!({"username":"Carrot Exception","content":content(upload,why,web_ok,response),"allowed_mentions":{"parse":[]},"flags":4});
    let result = (|| {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let branch = text(upload, "GitBranch", "unknown").replace('/', "__");
        let mut files = Vec::new();
        let path = upload.config.data_root.join("media/tmux.log");
        if path.exists() {
            let truncated = path.metadata()?.len() > 8 * 1024 * 1024;
            files.push((
                "files[0]",
                format!(
                    "{why}-{stamp}-{branch}{}.txt",
                    if truncated { "-truncated" } else { "" }
                ),
                "text/plain",
                attachment(&path)?,
            ));
        }
        let path = upload.config.data_root.join("toggle_values.json");
        if settings && path.exists() && path.metadata()?.len() <= 8 * 1024 * 1024 {
            files.push((
                "files[1]",
                format!("toggles-{stamp}.json"),
                "application/json",
                fs::read(path)?,
            ));
        }
        let (content_type, body) = if files.is_empty() {
            ("application/json".into(), serde_json::to_vec(&payload)?)
        } else {
            let boundary = uuid::Uuid::new_v4().simple().to_string();
            let mut body = Vec::new();
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\r\n{payload}\r\n").as_bytes());
            for (field, name, mime, bytes) in files {
                file(&mut body, &boundary, field, &name, mime, &bytes);
            }
            body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
            (format!("multipart/form-data; boundary={boundary}"), body)
        };
        Ok::<_, crate::Error>(openpilot_web_upload::post_bytes_socket(
            &url,
            &content_type,
            body,
            12,
        )?)
    })();
    match result {
        Ok(response) => {
            let ok = (200..300).contains(&response.status);
            eprintln!(
                "carrot_man discord tmux {}: status={} reason={why}",
                if ok { "sent" } else { "failed" },
                response.status
            );
            ok
        }
        Err(_) => {
            eprintln!("carrot_man discord tmux sending error");
            false
        }
    }
}
