use crate::{at::Apdu, codec::*, protocol, protocol::command, Result};
use serde_json::{json, Value};

pub fn split_bpp(bpp: &[u8]) -> Result<Vec<Vec<u8>>> {
    let root = tlvs(bpp)
        .into_iter()
        .find(|v| v.tag == 0xbf36)
        .ok_or_else(|| protocol("Invalid BoundProfilePackage"))?;
    let mut chunks = Vec::new();
    for part in tlvs(root.value) {
        match part.tag {
            0xbf23 => chunks.push(bpp[..root.value_start + part.end].to_vec()),
            0xa0 | 0xa2 => chunks.push(root.value[part.start..part.end].to_vec()),
            0xa1 | 0xa3 => {
                chunks.push(root.value[part.start..part.value_start].to_vec());
                for child in tlvs(part.value) {
                    chunks.push(part.value[child.start..child.end].to_vec());
                }
            }
            _ => {}
        }
    }
    Ok(chunks)
}
pub fn install_result(response: &[u8]) -> Result<Option<Value>> {
    let Some(root) = find(response, 0xbf37).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let Some(data) = find(root, 0xbf27).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let mut result = json!({"seqNumber":0,"success":false,"bppCommandId":null,"errorReason":null});
    if let Some(seq) = find(data, 0xbf2f)
        .and_then(|v| find(v, 0x80))
        .filter(|v| !v.is_empty())
    {
        result["seqNumber"] = json!(integer(seq)?);
    }
    if let Some(final_result) = find(data, 0xa2) {
        for part in tlvs(final_result) {
            match part.tag {
                0xa0 => result["success"] = json!(true),
                0xa1 => {
                    for (tag, key) in [(0x80, "bppCommandId"), (0x81, "errorReason")] {
                        if let Some(v) = find(part.value, tag).filter(|v| !v.is_empty()) {
                            result[key] = json!(integer(v)?);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(Some(result))
}
pub fn load_bpp(client: &mut impl Apdu, bpp: &str) -> Result<Value> {
    for chunk in split_bpp(&unb64(bpp)?)? {
        if let Some(result) = install_result(&command(client, &chunk)?)? {
            if result["success"] == true {
                return Ok(result);
            }
            if let Some(reason) = result["errorReason"].as_u64() {
                let message = match reason {
                    9 => "This eSIM profile is already installed on this device.".into(),
                    10 => "Not enough memory on the eUICC to install this profile.".into(),
                    12 => "Profile installation failed. The QR code may have already been used."
                        .into(),
                    _ => {
                        let id = result["bppCommandId"].as_u64();
                        let name = id
                            .and_then(|id| {
                                [
                                    "initialiseSecureChannel",
                                    "configureISDP",
                                    "storeMetadata",
                                    "storeMetadata2",
                                    "replaceSessionKeys",
                                    "loadProfileElements",
                                ]
                                .get(id as usize)
                                .copied()
                            })
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                format!("unknown({})", id.map_or("None".into(), |n| n.to_string()))
                            });
                        let name_reason = [
                            "incorrectInputValues",
                            "invalidSignature",
                            "invalidTransactionId",
                            "unsupportedCrtValues",
                            "unsupportedRemoteOperationType",
                            "unsupportedProfileClass",
                            "scp03tStructureError",
                            "scp03tSecurityError",
                            "iccidAlreadyExistsOnEuicc",
                            "insufficientMemoryForProfile",
                            "installInterrupted",
                            "peProcessingError",
                            "dataMismatch",
                            "invalidNAA",
                        ]
                        .get(reason.saturating_sub(1) as usize)
                        .filter(|_| reason > 0)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("unknown({reason})"));
                        format!("Profile installation failed at {name}: {name_reason}")
                    }
                };
                return Err(protocol(message));
            }
            break;
        }
    }
    Err(protocol(
        "Profile installation failed: no result from eUICC",
    ))
}
