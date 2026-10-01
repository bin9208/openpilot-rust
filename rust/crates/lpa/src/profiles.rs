use crate::{codec::*, protocol, Result};
use serde_json::{json, Value};

pub fn decode_list(blob: &[u8]) -> Result<Vec<Value>> {
    let root = require(blob, 0xbf2d, "ProfileInfoList")?;
    find(root, 0xa0).map_or(Ok(Vec::new()), |v| {
        tlvs(v)
            .into_iter()
            .filter(|v| v.tag == 0xe3)
            .map(|v| decode(v.value))
            .collect()
    })
}
pub fn decode(data: &[u8]) -> Result<Value> {
    let mut value = json!({"iccid":null,"isdpAid":null,"profileState":null,"profileNickname":null,"serviceProviderName":null,"profileName":null,"iconType":null,"icon":null,"profileClass":null});
    for t in tlvs(data) {
        let (key, v) = match t.tag {
            0x5a => ("iccid", json!(tbcd(t.value))),
            0x4f => ("isdpAid", json!(hex(t.value))),
            0x9f70 | 0x93 | 0x95 => {
                let n = *t
                    .value
                    .first()
                    .ok_or_else(|| protocol("empty profile enum"))?;
                match t.tag {
                    0x9f70 => (
                        "profileState",
                        json!(match n {
                            0 => "disabled",
                            1 => "enabled",
                            _ => "unknown",
                        }),
                    ),
                    0x93 => (
                        "iconType",
                        json!(match n {
                            0 => "jpeg",
                            1 => "png",
                            _ => "unknown",
                        }),
                    ),
                    _ => (
                        "profileClass",
                        json!(match n {
                            0 => "test",
                            1 => "provisioning",
                            2 => "operational",
                            _ => "unknown",
                        }),
                    ),
                }
            }
            0x90..=0x92 => {
                let s = utf8_ignore(t.value);
                (
                    ["profileNickname", "serviceProviderName", "profileName"]
                        [(t.tag - 0x90) as usize],
                    if s.is_empty() { Value::Null } else { json!(s) },
                )
            }
            0x94 => ("icon", json!(b64(t.value))),
            _ => continue,
        };
        value[key] = v;
    }
    Ok(value)
}
