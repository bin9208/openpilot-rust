use crate::{numeric, value, Commands, Error, JsonValue, JsonView, Number};
use num_bigint::BigInt;
use std::ffi::OsStr;

#[derive(Debug, Clone)]
pub struct WlanNetwork {
    pub mac: Option<String>,
    pub rss: Option<BigInt>,
}
#[derive(Debug, Clone)]
pub struct CellNetwork {
    pub mcc: BigInt,
    pub mnc: BigInt,
    pub cid: BigInt,
    pub pci: BigInt,
    pub earfcn: BigInt,
}
#[derive(Debug, Default, Clone)]
pub struct Networks {
    pub wlan: Option<Vec<WlanNetwork>>,
    pub lte: Option<CellNetwork>,
}
impl Networks {
    /// Uses the same decimal serialization boundary as Python JSON output;
    /// huge derived integers can exist in the typed result before serialization.
    pub fn to_json(&self) -> Result<JsonValue, Error> {
        let mut fields = Vec::new();
        if let Some(wlan) = &self.wlan {
            let entries: Result<Vec<_>, Error> = wlan
                .iter()
                .map(|network| {
                    let mac = match &network.mac {
                        Some(mac) => JsonValue::text(mac),
                        None => JsonValue::parse("null")?,
                    };
                    match &network.rss {
                        Some(rss) => value::object([
                            ("mac", mac),
                            ("rss", Number::Integer(rss.clone()).to_json()?),
                        ]),
                        None => value::object([("mac", mac)]),
                    }
                })
                .collect();
            fields.push(format!("\"wlan\":{}", value::array(&entries?)?.to_json()?));
        }
        if let Some(lte) = &self.lte {
            let nmr = value::object([
                ("pci", Number::Integer(lte.pci.clone()).to_json()?),
                ("earfcn", Number::Integer(lte.earfcn.clone()).to_json()?),
            ])?;
            let cell = value::object([
                ("mcc", Number::Integer(lte.mcc.clone()).to_json()?),
                ("mnc", Number::Integer(lte.mnc.clone()).to_json()?),
                ("cid", Number::Integer(lte.cid.clone()).to_json()?),
                ("nmr", value::array(&[nmr])?),
            ])?;
            fields.push(format!("\"lte\":{}", value::array(&[cell])?.to_json()?));
        }
        Ok(JsonValue::parse(&format!("{{{}}}", fields.join(",")))?)
    }
}
pub(crate) fn scan(commands: &impl Commands, interface: &str) -> Option<Vec<WlanNetwork>> {
    // iwlist.scan converts every command or parsing exception to None.
    commands
        .output(OsStr::new("iwlist"), &[interface.into(), "scan".into()])
        .and_then(|output| parse_scan(&output))
        .ok()
}
fn parse_scan(output: &str) -> Result<Vec<WlanNetwork>, Error> {
    let mut result = Vec::new();
    let mut mac = None;
    for line in output.split('\n') {
        if line.contains("Address") {
            if mac.is_some() {
                result.push(WlanNetwork {
                    mac: mac.take(),
                    rss: None,
                });
            }
            mac = Some(line.split(' ').next_back().unwrap_or("").to_owned());
        } else if line.contains("dBm") {
            let level = line.split("Signal level=").nth(1).ok_or(Error::Index)?;
            match numeric::integer(level.split(' ').next().unwrap_or(""), 10) {
                Ok(rss) => result.push(WlanNetwork {
                    mac: mac.take(),
                    rss: Some(rss),
                }),
                Err(Error::Value(_)) => continue,
                Err(error) => return Err(error),
            }
        }
    }
    if mac.is_some() {
        result.push(WlanNetwork { mac, rss: None });
    }
    Ok(result)
}
pub(crate) fn parse_lte(extra: &JsonValue) -> Result<Option<CellNetwork>, Error> {
    let found = match extra.view() {
        JsonView::Text(points) => points.windows(3).any(|slice| slice == [76, 84, 69]),
        JsonView::Array(values) => values.iter().any(|value| value.text_eq("LTE")),
        JsonView::Object(fields) => fields.iter().any(|(key, _)| *key == [76, 84, 69]),
        JsonView::Null | JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_) => {
            return Err(Error::Type("network extra is not iterable"))
        }
    };
    if !found {
        return Ok(None);
    }
    let JsonView::Text(points) = extra.view() else {
        return Err(Error::Attribute("network extra has no split method"));
    };
    let parts: Vec<_> = points.split(|&point| point == 44).collect();
    let integer = |index: usize, base| -> Result<BigInt, Error> {
        let text: Option<String> = parts
            .get(index)
            .ok_or(Error::Index)?
            .iter()
            .copied()
            .map(char::from_u32)
            .collect();
        numeric::integer(&text.ok_or(Error::Value("surrogate in integer"))?, base)
    };
    let parsed = (|| {
        Ok(CellNetwork {
            mcc: integer(3, 10)?,
            mnc: integer(4, 10)?,
            cid: integer(5, 16)?,
            pci: integer(6, 10)?,
            earfcn: integer(7, 10)?,
        })
    })();
    match parsed {
        Ok(cell) => Ok(Some(cell)),
        Err(Error::Value(_) | Error::Index) => Ok(None),
        Err(error) => Err(error),
    }
}
