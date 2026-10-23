use super::{default_key, parse_decimal, CliError, IntStringLimit, Options, Result};
use crate::argparse_scan::{self, OptionSpec};
use std::{io::Write, path::Path};

/// Bounded CLI grammar uses the existing native scanner, including long-option abbreviations.
/// Scanner diagnostics/help wrapping are not an argparse byte-for-byte port (see HANDOFF).
pub fn parse_options(args: &[String], source_root: &Path) -> Result<Option<Options>> {
    parse_options_with_limit(args, source_root, IntStringLimit::default())
}

/// Direct library entrypoint: policy is explicit; no environment access or global changes.
pub fn parse_options_with_limit(
    args: &[String],
    source_root: &Path,
    limit: IntStringLimit,
) -> Result<Option<Options>> {
    let parsed = argparse_scan::scan_with_positionals(
        args,
        &[
            OptionSpec::value("--host", false),
            OptionSpec::append("--port"),
            OptionSpec::value("--key", false),
            OptionSpec::flag("--debug", None),
        ],
        1,
    )?;
    // Scanner append mode records each port action consumed BEFORE help.
    // Validate all of them, including a bad value overwritten by a later good one.
    let mut port = String::from("22");
    if let Some(values) = parsed.append_values.get("--port") {
        for value in values {
            port = port_value(value, limit)?;
        }
    }
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(Options {
        device: parsed.positionals[0].clone(),
        host: parsed
            .values
            .get("--host")
            .cloned()
            .unwrap_or_else(|| "ssh.comma.ai".into()),
        port,
        key: match parsed.values.get("--key") {
            Some(key) => key.clone(),
            None => default_key(source_root)?,
        },
        debug: parsed.values.contains_key("--debug"),
    }))
}

fn port_value(input: &str, limit: IntStringLimit) -> Result<String> {
    // argparse int action wraps the underlying ValueError as an invalid int value.
    parse_decimal(input, limit).map_err(|_| {
        Box::<dyn std::error::Error>::from(CliError(format!("invalid int value: {input:?}")))
    })
}

pub fn ssh_cli() -> Result<i32> {
    // Original interpreter validates this startup setting before script/help/API.
    // Scope the snapshot to this SSH entrypoint, not process-global/library state.
    let limit =
        IntStringLimit::from_startup_env(std::env::var_os("PYTHONINTMAXSTRDIGITS").as_deref())?;
    let argv = std::env::args().collect::<Vec<_>>();
    // Matches original BASEDIR: this checkout's root, independent of working directory.
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let source_root = std::fs::canonicalize(source_root)?;
    let options = match parse_options_with_limit(&argv[1..], &source_root, limit) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!(
                "usage: {} [-h] [--host HOST] [--port PORT] [--key KEY] [--debug] device\n\nA helper for connecting to devices over the comma prime SSH proxy.\nAdding your SSH key to your SSH config is recommended for more convenient use; see https://docs.comma.ai/how-to/connect-to-comma/.\n\npositional arguments:\n  device       device name or dongle id\n\noptions:\n  -h, --help   show this help message and exit\n  --host HOST  ssh jump server host\n  --port PORT  ssh jump server port\n  --key KEY    ssh key\n  --debug      enable debug output",
                argv[0]
            );
            return Ok(0);
        }
        Err(error) => {
            eprintln!(
                "usage: {} [-h] [--host HOST] [--port PORT] [--key KEY] [--debug] device\n{}: error: {error}",
                argv[0], argv[0]
            );
            return Ok(2);
        }
    };
    let root = if Path::new("/TICI").is_file() {
        super::config_root(Path::new(""), "", true)
    } else {
        #[allow(deprecated)]
        let home =
            std::env::home_dir().ok_or("home directory unavailable for auth configuration")?;
        let prefix = std::env::var("OPENPILOT_PREFIX").unwrap_or_default();
        super::config_root(&home, &prefix, false)
    };
    let token = super::get_token_at(&root);
    let host = std::env::var("API_HOST").unwrap_or_else(|_| "https://api.commadotai.com".into());
    match super::prepare(&mut super::http::NativeTransport, &host, &token, &options)? {
        Ok(plan) => {
            plan.exec()?;
            Ok(0)
        }
        Err(failure) => {
            std::io::stderr().write_all(failure.stderr.as_bytes())?;
            Ok(1)
        }
    }
}

pub fn setup_cli() -> Result<i32> {
    let argv = std::env::args().collect::<Vec<_>>();
    // Original sys.argv indexing: extra args ignored; --help is a literal username.
    let Some(username) = argv.get(1) else {
        println!("{} <github username>", argv[0]);
        return Ok(1);
    };
    let message = super::setup_keys(&mut super::http::NativeTransport, username, || {
        Ok(openpilot_params::Params::for_runtime()?)
    })?;
    std::io::stdout().write_all(message.as_bytes())?;
    Ok(0)
}
