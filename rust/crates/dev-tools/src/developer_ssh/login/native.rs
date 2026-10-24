//! Explicit browser entrypoint; nothing executes during import or library tests.
use super::{auth_redirect_link, cli, login_with, Callback, LoginIo, LoginStatus, Provider};
use crate::developer_ssh::{
    config_root, get_token_at, http::NativeTransport, set_token_at, Get, RequestsText, Result,
    TextDecoder, Transport,
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

fn token_root() -> Result<std::path::PathBuf> {
    if Path::new("/TICI").is_file() {
        return Ok(config_root(Path::new(""), "", true));
    }
    #[allow(deprecated)]
    let home = std::env::home_dir().ok_or("home directory unavailable for auth configuration")?;
    Ok(config_root(
        &home,
        &std::env::var("OPENPILOT_PREFIX").unwrap_or_default(),
        false,
    ))
}
fn save_token(token: &str) -> Result<()> {
    set_token_at(&token_root()?, &JsonValue::text(token))
}
fn final_request(api_host: &str, token: &JsonValue) -> Result<Get> {
    Ok(Get {
        // Source CommaApi.request adds '/', while this main call passes '/v1/me'.
        url: format!("{api_host}//v1/me"),
        authorization: if crate::developer_ssh::truth(token) {
            Some(format!(
                "JWT {}",
                token
                    .to_utf8()
                    .ok_or("can only concatenate str to JWT prefix")?
            ))
        } else {
            None
        },
        user_agent: "OpenpilotTools",
        // The final main call is distinct from login's bounded verification call.
        timeout: None,
    })
}
fn final_api_json(response: crate::developer_ssh::Response) -> Result<JsonValue> {
    let value = JsonValue::parse(&RequestsText.decode(&response.body, &response.content_type)?)?;
    if let Some(error) = value.get("error").filter(crate::developer_ssh::truth) {
        if matches!(response.status, 401 | 403) {
            return Err(cli::Unauthorized.into());
        }
        let description = match value.get("description") {
            Some(value) => value.to_utf8().ok_or("API description must be str")?,
            None => crate::developer_ssh::python_str(&error)?,
        };
        return Err(format!("{}:{description}", response.status).into());
    }
    Ok(value)
}
struct NativeCli {
    login: NativeLogin,
}
impl cli::CliIo for NativeCli {
    fn login(&mut self, provider: Provider) -> LoginStatus {
        login_with(
            &mut self.login,
            provider,
            Duration::from_secs(180),
            save_token,
        )
    }
    fn set_token(&mut self, token: &str) -> Result<()> {
        save_token(token)
    }
    fn get_token(&mut self) -> Result<JsonValue> {
        // get_token() suppresses all discovery/read/parse errors, returning None.
        Ok(token_root()
            .map(|root| get_token_at(&root))
            .unwrap_or_else(|_| JsonValue::parse("null").expect("JSON null")))
    }
    fn me(&mut self, token: &JsonValue) -> Result<JsonValue> {
        final_api_json(NativeTransport.get(&final_request(&self.login.api_host, token)?)?)
    }
}
/// Full normal/JWT/JSON candidate; login.rs retains the historical JSON symbol alias.
pub fn auth_cli() -> Result<i32> {
    let argv = std::env::args().collect::<Vec<_>>();
    let prog = Path::new(&argv[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&argv[0]);
    let mut io = NativeCli {
        login: NativeLogin {
            listener: None,
            callback: Arc::new(Mutex::new(Callback::default())),
            browser: None,
            clock: Instant::now(),
            api_host: std::env::var("API_HOST")
                .unwrap_or_else(|_| "https://api.commadotai.com".into()),
        },
    };
    cli::run_cli_with(
        prog,
        &argv[1..],
        &mut io,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;

    // Every client and accepted stream belongs to a fresh private loopback listener.
    // No browser/start/API/token/config/stdin entrypoint is called by these tests.
    fn listener() -> TcpListener {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    }

    fn client(listener: &TcpListener) -> TcpStream {
        let stream =
            TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(2))
                .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
    }

    fn response(client: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        client.read_to_end(&mut bytes).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    fn request(listener: &TcpListener, callback: &Arc<Mutex<Callback>>, target: &str) -> String {
        let mut client = client(listener);
        write!(client, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        // connect_timeout completed the handshake; the one private connection is queued.
        let (stream, _) = listener.accept().unwrap();
        handle_callback(stream, Arc::clone(callback)).unwrap();
        response(&mut client)
    }

    fn saved(callback: &Arc<Mutex<Callback>>, code: &str, provider: &str) {
        let callback = callback.lock().unwrap();
        assert_eq!(callback.query.len(), 2);
        assert_eq!(callback.query["code"], [code]);
        assert_eq!(callback.query["provider"], [provider]);
    }

    #[test]
    fn final_me_request_retains_source_url_timeout_and_api_error_rules() {
        let request = final_request("https://example.invalid/", &JsonValue::text("inert")).unwrap();
        assert_eq!(request.url, "https://example.invalid///v1/me");
        assert_eq!(request.timeout, None);
        assert_eq!(request.authorization.as_deref(), Some("JWT inert"));
        for token in ["null", "false", "0", "\"\"", "[]", "{}"] {
            assert!(
                final_request("https://example.invalid", &JsonValue::parse(token).unwrap())
                    .unwrap()
                    .authorization
                    .is_none()
            );
        }
        assert!(
            final_request("https://example.invalid", &JsonValue::parse("123").unwrap()).is_err()
        );
        let response = |status, body: &str| crate::developer_ssh::Response {
            status,
            body: body.as_bytes().to_vec(),
            content_type: "application/json; charset=utf-8".into(),
        };
        for status in [401, 403] {
            assert!(final_api_json(response(status, r#"{"error":"denied"}"#))
                .err()
                .unwrap()
                .is::<cli::Unauthorized>());
            assert!(final_api_json(response(status, r#"{"error":false}"#)).is_ok());
        }
        let failure = final_api_json(response(
            500,
            r#"{"error":"bad","description":"synthetic"}"#,
        ))
        .err()
        .unwrap();
        assert!(!failure.is::<cli::Unauthorized>());
        assert_eq!(failure.to_string(), "500:synthetic");
    }

    #[test]
    fn socket_routes_return_status_and_preserve_first_auth_result() {
        let listener = listener();
        // Each path must independently admit a first result, then resist replacement.
        for first_path in ["/auth", "/auth/"] {
            let callback = Arc::new(Mutex::new(Callback::default()));
            let noise = request(&listener, &callback, "/favicon.ico?error=ignored");
            assert!(noise.starts_with("HTTP/1.0 204 No Content\r\n"));
            assert!(noise.ends_with("\r\n\r\n"));
            assert!(callback.lock().unwrap().is_empty());

            let accepted = request(
                &listener,
                &callback,
                &format!("{first_path}?code=first%2Bcode&provider=g"),
            );
            assert!(accepted.starts_with("HTTP/1.0 200 OK\r\n"));
            assert!(accepted.ends_with(
                "Sign-in received. You can close this tab and return to Cabana or your terminal."
            ));
            saved(&callback, "first+code", "g");

            let later = request(&listener, &callback, "/auth/?error=declined");
            assert!(later.starts_with("HTTP/1.0 200 OK\r\n"));
            saved(&callback, "first+code", "g");
        }
    }

    #[test]
    fn incomplete_preconnection_does_not_block_second_callback() {
        let listener = listener();
        let mut incomplete = client(&listener);
        incomplete
            .write_all(b"GET /auth?code=late&provider=h HTTP/1.1\r\nHost: localhost")
            .unwrap();
        let mut login = NativeLogin {
            listener: Some(listener),
            callback: Arc::new(Mutex::new(Callback::default())),
            browser: None,
            clock: Instant::now(),
            api_host: String::new(),
        };
        // Only poll is used: at most two production handler threads, each retaining
        // its existing one-second accepted-stream timeout. No test thread/process.
        assert!(login
            .poll_callback(Duration::from_millis(100))
            .unwrap()
            .is_empty());
        let mut valid = client(login.listener.as_ref().unwrap());
        valid
            .write_all(b"GET /auth/?code=ready&provider=g HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        login.poll_callback(Duration::from_millis(100)).unwrap();
        assert!(response(&mut valid).starts_with("HTTP/1.0 200 OK\r\n"));
        saved(&login.callback, "ready", "g");

        // Complete the first request only AFTER the second response. A synchronous
        // poll/handler regression would time out/close it, so eventual success alone
        // cannot make this test pass. Read both responses to EOF to finish both workers.
        incomplete.write_all(b"\r\n\r\n").unwrap();
        assert!(response(&mut incomplete).starts_with("HTTP/1.0 200 OK\r\n"));
        saved(&login.callback, "ready", "g");
        assert!(login.browser.is_none());
    }

    #[test]
    fn closed_client_does_not_discard_received_callback() {
        let listener = listener();
        let callback = Arc::new(Mutex::new(Callback::default()));
        let mut closed = client(&listener);
        closed
            .write_all(b"GET /auth?code=retained&provider=a HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let (stream, _) = listener.accept().unwrap();
        closed.shutdown(Shutdown::Both).unwrap();
        drop(closed);

        // Kernels may accept the final write or report a disconnected peer. Both
        // outcomes must leave the already parsed result intact; no forced RST claim.
        let _ = handle_callback(stream, Arc::clone(&callback));
        saved(&callback, "retained", "a");
        assert!(
            request(&listener, &callback, "/auth/?code=replacement&provider=g")
                .starts_with("HTTP/1.0 200 OK\r\n")
        );
        saved(&callback, "retained", "a");
    }
}
