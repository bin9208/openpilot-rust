//! Explicit browser entrypoint; nothing executes during import or library tests.
use super::{auth_redirect_link, login_with, parse_json_options, Callback, LoginIo, Provider};
use crate::developer_ssh::{
    config_root, http::NativeTransport, set_token_at, Get, RequestsText, Result, TextDecoder,
    Transport,
};
use openpilot_log_format::JsonValue;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct NativeLogin {
    listener: Option<TcpListener>,
    callback: Arc<Mutex<Callback>>,
    browser: Option<Child>,
    clock: Instant,
    api_host: String,
}
impl NativeLogin {
    fn api(&self, endpoint: &str, token: Option<&str>) -> Get {
        Get {
            url: format!("{}/{endpoint}", self.api_host),
            authorization: token.map(|token| format!("JWT {token}")),
            user_agent: "OpenpilotTools",
            timeout: Some(Duration::from_secs(30)),
        }
    }
}
fn api_json(response: crate::developer_ssh::Response) -> Result<JsonValue> {
    let text = RequestsText.decode(&response.body, &response.content_type)?;
    let value = JsonValue::parse(&text)?;
    if value
        .get("error")
        .as_ref()
        .is_some_and(crate::developer_ssh::truth)
    {
        // login() catches all dependency errors; never log API bodies or tokens.
        return Err("authentication API returned an error".into());
    }
    Ok(value)
}
impl LoginIo for NativeLogin {
    fn start(&mut self, provider: Provider) -> Result<()> {
        // Python ThreadingHTTPServer uses AF_INET localhost with an ephemeral port.
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let url = auth_redirect_link(provider, listener.local_addr()?.port());
        self.listener = Some(listener);
        eprintln!("To sign in, use your browser and navigate to {url}");
        self.browser = Some(
            Command::new(if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            })
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
        );
        Ok(())
    }
    fn now(&self) -> Duration {
        self.clock.elapsed()
    }
    fn poll_callback(&mut self, wait: Duration) -> Result<Callback> {
        let deadline = Instant::now() + wait;
        let listener = self.listener.as_ref().ok_or("login listener not started")?;
        loop {
            if !self
                .callback
                .lock()
                .map_err(|_| "callback lock poisoned")?
                .is_empty()
            {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let callback = Arc::clone(&self.callback);
                    // Preconnections/incomplete requests cannot block a valid second callback.
                    std::thread::spawn(move || {
                        let _ = handle_callback(stream, callback);
                    });
                    // Match handle_request(): after accepting one request the caller checks
                    // callback/browser state, while the bounded handler runs independently.
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    std::thread::sleep(remaining.min(Duration::from_millis(5)));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(self
            .callback
            .lock()
            .map_err(|_| "callback lock poisoned")?
            .clone())
    }
    fn browser_failed(&mut self) -> Result<bool> {
        Ok(self
            .browser
            .as_mut()
            .ok_or("browser not started")?
            .try_wait()?
            .is_some_and(|status| !status.success()))
    }
    fn exchange(&mut self, code: &str, provider: &str) -> Result<JsonValue> {
        let response = NativeTransport.post_form(
            &self.api("v2/auth/", None),
            &[("code", code), ("provider", provider)],
        )?;
        let value = api_json(response)?;
        if !value.is_object() {
            return Err("authentication response is not an object".into());
        }
        Ok(value
            .get("access_token")
            .unwrap_or_else(|| JsonValue::parse("null").expect("JSON null")))
    }
    fn verify(&mut self, token: &str) -> Result<()> {
        api_json(NativeTransport.get(&self.api("v1/me", Some(token)))?)?;
        Ok(())
    }
}
fn handle_callback(mut stream: TcpStream, callback: Arc<Mutex<Callback>>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    // Match the source's bounded request line, without logging a callback's secret code.
    reader.by_ref().take(65537).read_line(&mut line)?;
    if line.len() > 65536 || !line.ends_with('\n') {
        return Ok(());
    }
    let mut fields = line.split_whitespace();
    let method = fields.next().unwrap_or("");
    let target = fields.next().unwrap_or("").to_owned();
    let mut complete = false;
    for _ in 0..100 {
        let mut header = Vec::new();
        reader.by_ref().take(65537).read_until(b'\n', &mut header)?;
        if header.len() > 65536 || !header.ends_with(b"\n") {
            return Ok(());
        }
        if header == b"\r\n" || header == b"\n" {
            complete = true;
            break;
        }
    }
    if !complete {
        return Ok(());
    }
    let status = if method == "GET" {
        callback
            .lock()
            .map_err(|_| "callback lock poisoned")?
            .receive(&target)
    } else {
        501
    };
    let (reason, body) = match status {
        200 => (
            "OK",
            "Sign-in received. You can close this tab and return to Cabana or your terminal.",
        ),
        204 => ("No Content", ""),
        _ => ("Not Implemented", ""),
    };
    // The accepted result is stored before writing; browser disconnect cannot discard it.
    write!(
        stream,
        "HTTP/1.0 {status} {reason}\r\nContent-type: text/plain\r\n\r\n{body}"
    )?;
    Ok(())
}

pub fn auth_json_cli() -> Result<i32> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let provider = match parse_json_options(&args) {
        Ok(Some(provider)) => provider,
        Ok(None) => {
            println!(
                "usage: developer-auth-json [google|apple|github] --json\nBrowser sign-in status as JSON; plain/JWT mode remains in auth.py."
            );
            return Ok(0);
        }
        Err(error) => {
            eprintln!("{error}");
            return Ok(2);
        }
    };
    let mut io = NativeLogin {
        listener: None,
        callback: Arc::new(Mutex::new(Callback::default())),
        browser: None,
        clock: Instant::now(),
        api_host: std::env::var("API_HOST").unwrap_or_else(|_| "https://api.commadotai.com".into()),
    };
    let status = login_with(&mut io, provider, Duration::from_secs(180), |token| {
        let root = if Path::new("/TICI").is_file() {
            config_root(Path::new(""), "", true)
        } else {
            #[allow(deprecated)]
            let home =
                std::env::home_dir().ok_or("home directory unavailable for auth configuration")?;
            config_root(
                &home,
                &std::env::var("OPENPILOT_PREFIX").unwrap_or_default(),
                false,
            )
        };
        set_token_at(&root, &JsonValue::text(token))
    });
    println!("{}", status.to_json());
    // Source --json exits zero for both success/error status; parse errors exit two.
    Ok(0)
}
