use openpilot_lpa::{at::Config, service::Lpa, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    List,
    Active,
    Delete {
        iccid: String,
    },
    Nickname {
        iccid: String,
        nickname: String,
    },
    Switch {
        iccid: String,
    },
    Download {
        activation: String,
        nickname: Option<String>,
    },
    Notifications,
    IsEuicc,
}
fn run() -> Result<Value> {
    let mut args = std::env::args().skip(1);
    let config = match args.next().as_deref() {
        None => Config::default(),
        Some("--config") => serde_json::from_reader(std::fs::File::open(
            args.next()
                .ok_or_else(|| Error::Value("--config requires a path".into()))?,
        )?)?,
        Some("--help") => {
            println!("openpilot-lpa [--config PATH]\nReads one typed JSON operation on stdin. Defaults address the device modem; fixture configuration must use owned paths.");
            return Ok(Value::Null);
        }
        _ => return Err(Error::Value("unknown argument".into())),
    };
    if args.next().is_some() {
        return Err(Error::Value("unexpected argument".into()));
    }
    let operation = serde_json::from_reader(std::io::stdin())?;
    let mut lpa = Lpa::new(config);
    Ok(match operation {
        Operation::List => json!(lpa.list_profiles()?),
        Operation::Active => json!(lpa.get_active_profile()),
        Operation::Delete { iccid } => {
            lpa.delete_profile(&iccid)?;
            Value::Null
        }
        Operation::Nickname { iccid, nickname } => {
            lpa.nickname_profile(&iccid, &nickname)?;
            Value::Null
        }
        Operation::Switch { iccid } => {
            lpa.switch_profile(&iccid)?;
            Value::Null
        }
        Operation::Download {
            activation,
            nickname,
        } => {
            lpa.download_profile(&activation, nickname.as_deref())?;
            Value::Null
        }
        Operation::Notifications => {
            lpa.process_notifications()?;
            Value::Null
        }
        Operation::IsEuicc => json!(lpa.is_euicc()?),
    })
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(value) => {
            println!("{value}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("lpa: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
