use openpilot_lpa::{
    at::{Apdu, Config},
    bpp, codec,
    http::{Es9, Http},
    protocol,
    service::Lpa,
    Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};
#[derive(Deserialize)]
struct Request {
    config: Config,
    ca: PathBuf,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
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
    Query {
        command: String,
    },
    Apdu {
        data: String,
    },
    Command {
        data: String,
    },
    Codec {
        data: String,
        digits: String,
        activation: String,
    },
    Prepare {
        signed: String,
        signature: String,
        certificate: String,
        cc: Option<String>,
    },
    Http {
        address: String,
    },
}
struct FreshHttp(Vec<u8>);
impl Es9 for FreshHttp {
    fn request(&mut self, a: &str, e: &str, p: Value, prefix: &str) -> Result<Value> {
        Http::with_roots(&self.0, Duration::from_secs(2))?.request(a, e, p, prefix)
    }
}
fn execute(lpa: &mut Lpa, pem: &[u8], op: Operation) -> Result<Value> {
    Ok(match op {
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
            let mut http = Http::with_roots(pem, Duration::from_secs(2))?;
            lpa.download_with_http(&mut http, &activation, nickname.as_deref())?;
            Value::Null
        }
        Operation::Notifications => {
            lpa.process_notifications_with_http(&mut FreshHttp(pem.into()))?;
            Value::Null
        }
        Operation::IsEuicc => json!(lpa.is_euicc()?),
        Operation::Query { command } => json!(lpa.client.query(&command)?),
        Operation::Apdu { data } => {
            let (data, a, b) = lpa.client.send_apdu(&codec::unhex(&data)?)?;
            json!([codec::hex(&data), a, b])
        }
        Operation::Command { data } => json!(codec::hex(&protocol::command(
            &mut lpa.client,
            &codec::unhex(&data)?
        )?)),
        Operation::Prepare {
            signed,
            signature,
            certificate,
            cc,
        } => json!(protocol::prepare(
            &mut lpa.client,
            &signed,
            &signature,
            &certificate,
            cc.as_deref()
        )?),
        Operation::Http { address } => {
            let mut http = Http::with_roots(pem, Duration::from_secs(2))?;
            http.request(&address, "fixture", json!({}), "Request")?
        }
        Operation::Codec {
            data,
            digits,
            activation,
        } => {
            let bytes = codec::unhex(&data)?;
            let tags: Vec<_> = codec::tlvs(&bytes)
                .into_iter()
                .map(|t| json!([t.tag, codec::hex(t.value), t.start, t.end]))
                .collect();
            json!({"tlv":tags,"tbcd":codec::hex(&codec::to_tbcd(&digits)),"digits":codec::tbcd(&codec::to_tbcd(&digits)),"b64":codec::b64(&bytes),"activation":codec::activation(&activation)?,"bpp":bpp::split_bpp(&bytes)?.iter().map(|s|codec::hex(s)).collect::<Vec<_>>()})
        }
    })
}
fn main() -> Result<()> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let pem = std::fs::read(request.ca)?;
    let mut lpa = Lpa::new(request.config);
    let mut rows = Vec::new();
    for op in request.operations {
        rows.push(match execute(&mut lpa, &pem, op) {
            Ok(v) => json!({"result":v}),
            Err(e) => json!({"error":e.to_string()}),
        });
    }
    lpa.client.close()?;
    serde_json::to_writer(std::io::stdout(), &rows).map_err(Error::from)
}
