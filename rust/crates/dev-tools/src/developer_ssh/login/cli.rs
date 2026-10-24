//! Original auth.py main flow; injectable effects keep qualification entirely inert.
use super::{pretty, LoginStatus, Provider};
use crate::developer_ssh::Result;
use openpilot_log_format::JsonValue;
use std::io::Write;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(super) struct Unauthorized;
impl std::fmt::Display for Unauthorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Unauthorized. Authenticate with tools/lib/auth.py")
    }
}
impl std::error::Error for Unauthorized {}

pub(super) trait CliIo {
    fn login(&mut self, provider: Provider) -> LoginStatus;
    fn set_token(&mut self, token: &str) -> Result<()>;
    fn get_token(&mut self) -> Result<JsonValue>;
    fn me(&mut self, token: &JsonValue) -> Result<JsonValue>;
}
#[derive(Debug)]
enum Method {
    Browser(Provider),
    Jwt,
}
struct Options {
    method: Method,
    jwt: Option<String>,
    json: bool,
}
enum Parsed {
    Help,
    Options(Options),
}

fn usage(prog: &str) -> String {
    format!("usage: {prog} [-h] [--json] [{{google,apple,github,jwt}}] [jwt]\n")
}
fn help(prog: &str) -> String {
    format!("{}\nLogin to your comma account\n\npositional arguments:\n  {{google,apple,github,jwt}}\n  jwt\n\noptions:\n  -h, --help            show this help message and exit\n  --json                Return browser sign-in status as JSON\n", usage(prog))
}
fn quoted(value: &str) -> Result<String> {
    let wrapped = JsonValue::parse(&format!("[{}]", JsonValue::text(value).to_json()?))?;
    let points = openpilot_runtime_version::python_str(&wrapped)?;
    points[1..points.len() - 1]
        .iter()
        .map(|&p| char::from_u32(p).ok_or_else(|| "unencodable argument repr".into()))
        .collect()
}
#[derive(Clone)]
enum Arg {
    Positional,
    Delimiter,
    Help(Option<String>),
    Json(Option<String>),
    Unknown,
}
fn classify(value: &str, literal: bool) -> Arg {
    if literal {
        return Arg::Positional;
    }
    if value == "--" {
        return Arg::Delimiter;
    }
    if value.starts_with("-h") && !value.starts_with("--") {
        // All short actions are collected before argparse executes help. Repeated
        // h flags are valid; '=' or a '-' tail is an ignored-explicit-argument error.
        let tail = value[2..].trim_start_matches('h');
        let explicit = tail
            .strip_prefix('=')
            .map(str::to_owned)
            .or_else(|| tail.starts_with('-').then(|| tail.to_owned()));
        return Arg::Help(explicit);
    }
    let (name, explicit) = value
        .split_once('=')
        .map_or((value, None), |(n, v)| (n, Some(v.to_owned())));
    if matches!(name, "-h" | "--help") || name.starts_with("--") && "--help".starts_with(name) {
        return Arg::Help(explicit);
    }
    if name.starts_with("--") && "--json".starts_with(name) {
        return Arg::Json(explicit);
    }
    // Source negative-number classification and embedded-space positional rule.
    if !value.starts_with('-')
        || value == "-"
        || value.contains(' ')
        || crate::argparse_scan::negative_number(value)
    {
        Arg::Positional
    } else {
        Arg::Unknown
    }
}
fn parse(args: &[String]) -> Result<std::result::Result<Parsed, String>> {
    let mut literal = false;
    let classified = args
        .iter()
        .map(|value| {
            let kind = classify(value, literal);
            if matches!(kind, Arg::Delimiter) {
                literal = true;
            }
            kind
        })
        .collect::<Vec<_>>();
    let (mut index, mut consumed_positionals, mut json) = (0, false, false);
    let (mut method, mut jwt) = ("google".to_owned(), None);
    let mut extras = vec![];
    while index < args.len() {
        match &classified[index] {
            Arg::Help(explicit) | Arg::Json(explicit) => {
                let is_help = matches!(classified[index], Arg::Help(_));
                if let Some(explicit) = explicit {
                    let option = if is_help { "-h/--help" } else { "--json" };
                    return Ok(Err(format!(
                        "argument {option}: ignored explicit argument {}",
                        quoted(explicit)?
                    )));
                }
                if is_help {
                    return Ok(Ok(Parsed::Help));
                }
                json = true;
                index += 1;
            }
            Arg::Unknown => {
                extras.push(args[index].clone());
                index += 1;
            }
            Arg::Positional | Arg::Delimiter if !consumed_positionals => {
                // Both '?' positional actions run at the first positional segment.
                // A later segment after --json is extra even if jwt was defaulted.
                consumed_positionals = true;
                for slot in 0..2 {
                    while index < args.len() && matches!(classified[index], Arg::Delimiter) {
                        index += 1;
                    }
                    if index < args.len() && matches!(classified[index], Arg::Positional) {
                        let value = args[index].clone();
                        index += 1;
                        if slot == 0 {
                            if !matches!(value.as_str(), "google" | "apple" | "github" | "jwt") {
                                return Ok(Err(format!("argument method: invalid choice: {} (choose from 'google', 'apple', 'github', 'jwt')", quoted(&value)?)));
                            }
                            method = value;
                        } else {
                            jwt = Some(value);
                        }
                    }
                    while index < args.len() && matches!(classified[index], Arg::Delimiter) {
                        index += 1;
                    }
                }
            }
            _ => {
                extras.push(args[index].clone());
                index += 1;
            }
        }
    }
    if !extras.is_empty() {
        return Ok(Err(format!("unrecognized arguments: {}", extras.join(" "))));
    }
    if json && method == "jwt" {
        return Ok(Err("--json requires a browser sign-in provider".into()));
    }
    Ok(Ok(Parsed::Options(Options {
        method: match method.as_str() {
            "google" => Method::Browser(Provider::Google),
            "apple" => Method::Browser(Provider::Apple),
            "github" => Method::Browser(Provider::Github),
            _ => Method::Jwt,
        },
        jwt,
        json,
    })))
}
pub(super) fn run_cli_with(
    prog: &str,
    args: &[String],
    io: &mut impl CliIo,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<i32> {
    let options = match parse(args)? {
        Ok(Parsed::Help) => {
            out.write_all(help(prog).as_bytes())?;
            return Ok(0);
        }
        Ok(Parsed::Options(options)) => options,
        Err(message) => {
            writeln!(err, "{}{prog}: error: {message}", usage(prog))?;
            return Ok(2);
        }
    };
    match options.method {
        Method::Browser(provider) => {
            let status = io.login(provider);
            if options.json {
                writeln!(out, "{}", status.to_json())?;
                return Ok(0);
            }
            if let LoginStatus::Error(message) = status {
                writeln!(err, "{message}")?;
                return Ok(1);
            }
        }
        Method::Jwt => {
            let Some(token) = options.jwt else {
                writeln!(out, "method JWT selected, but no JWT was provided")?;
                return Ok(1);
            };
            // Source deliberately persists even an empty JWT before final verification.
            io.set_token(&token)?;
        }
    }
    let token = io.get_token()?;
    let me = match io.me(&token) {
        Ok(me) => me,
        Err(error) if error.is::<Unauthorized>() => {
            writeln!(out, "Got invalid JWT")?;
            return Ok(1);
        }
        Err(error) => return Err(error),
    };
    writeln!(out, "Authenticated!")?;
    writeln!(out, "{}", pretty::pformat(&me)?)?;
    Ok(0)
}
