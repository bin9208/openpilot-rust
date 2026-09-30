use crate::{at::Apdu, codec::*, http::Es9, protocol, Error, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn command(client: &mut impl Apdu, data: &[u8]) -> Result<Vec<u8>> {
    let mut response = Vec::new();
    for (sequence, chunk) in data.chunks(120).enumerate() {
        let mut apdu = vec![
            0x80,
            0xe2,
            if (sequence + 1) * 120 >= data.len() {
                0x91
            } else {
                0x11
            },
            sequence as u8,
            chunk.len() as u8,
        ];
        apdu.extend(chunk);
        let (segment, mut sw1, mut sw2) = client.send_apdu(&apdu)?;
        response.extend(segment);
        loop {
            if sw1 == 0x61 {
                let (segment, a, b) = client.send_apdu(&[0x80, 0xc0, 0, 0, sw2])?;
                response.extend(segment);
                sw1 = a;
                sw2 = b;
                continue;
            }
            if sw1 & 0xf0 == 0x90 {
                break;
            }
            return Err(protocol(format!("APDU failed with SW={sw1:02X}{sw2:02X}")));
        }
    }
    Ok(response)
}
pub fn status(response: &[u8], tag: u64, label: &str, status_label: &str) -> Result<u8> {
    require(require(response, tag, label)?, 0x80, status_label)?
        .first()
        .copied()
        .ok_or_else(|| protocol("empty status"))
}
pub fn list_profiles(client: &mut impl Apdu) -> Result<Vec<Value>> {
    crate::profiles::decode_list(&command(client, &encode(0xbf2d, &[]))?)
}
pub fn nickname(client: &mut impl Apdu, iccid: &str, nickname: &str) -> Result<()> {
    if nickname.len() > 64 {
        return Err(Error::Value(
            "Profile nickname must be 64 bytes or less".into(),
        ));
    }
    let mut content = encode(0x5a, &to_tbcd(iccid));
    content.extend(encode(0x90, nickname.as_bytes()));
    let code = status(
        &command(client, &encode(0xbf29, &content))?,
        0xbf29,
        "SetNicknameResponse",
        "SetNickname status",
    )?;
    match code {
        0 => Ok(()),
        1 => Err(Error::Lpa(format!("profile {iccid} not found"))),
        _ => Err(protocol(format!(
            "SetNickname failed with status 0x{code:02X}"
        ))),
    }
}
pub fn profile_error(code: u8) -> &'static str {
    match code {
        1 => "iccidOrAidNotFound",
        2 => "profileNotInDisabledState",
        3 => "disallowedByPolicy",
        4 => "wrongProfileReenabling",
        5 => "catBusy",
        6 => "undefinedError",
        _ => "unknown",
    }
}
pub fn enable(client: &mut impl Apdu, iccid: &str) -> Result<u8> {
    let mut inner = encode(0xa0, &encode(0x5a, &to_tbcd(iccid)));
    inner.extend([1, 1, 1]);
    status(
        &command(client, &encode(0xbf31, &inner))?,
        0xbf31,
        "EnableProfileResponse",
        "EnableProfile status",
    )
}
pub fn challenge_and_info(client: &mut impl Apdu) -> Result<(Vec<u8>, Vec<u8>)> {
    let response = command(client, &encode(0xbf2e, &[]))?;
    let challenge = require(
        require(&response, 0xbf2e, "GetEuiccDataResponse")?,
        0x80,
        "challenge in response",
    )?
    .to_vec();
    let info = command(client, &encode(0xbf20, &[]))?;
    require(&info, 0xbf20, "GetEuiccInfo1Response")?;
    Ok((challenge, info))
}
pub fn authenticate_server(
    client: &mut impl Apdu,
    signed: &str,
    sig: &str,
    key: &str,
    cert: &str,
    matching: &str,
) -> Result<String> {
    let mut device = encode(0x80, &[0x35, 0x29, 0x06, 0x11]);
    device.extend(encode(0xa1, &[]));
    let mut context = encode(0x80, matching.as_bytes());
    context.extend(encode(0xa1, &device));
    let mut content = unb64(signed)?;
    content.extend(unb64(sig)?);
    content.extend(unb64(key)?);
    content.extend(unb64(cert)?);
    content.extend(encode(0xa0, &context));
    let response = command(client, &encode(0xbf38, &content))?;
    let root = require(&response, 0xbf38, "AuthenticateServerResponse")?;
    if let Some(error) = find(root, 0xa1) {
        let code = integer(error)?;
        let name = match code {
            1 => "eUICCVerificationFailed",
            2 => "eUICCCertificateExpired",
            3 => "eUICCCertificateRevoked",
            5 => "invalidServerSignature",
            6 => "euiccCiPKUnknown",
            10 => "matchingIdRefused",
            16 => "insufficientMemory",
            _ => "unknown",
        };
        return Err(protocol(format!(
            "AuthenticateServer rejected by eUICC: {name} (0x{code:02X})"
        )));
    }
    Ok(b64(&response))
}
pub fn prepare(
    client: &mut impl Apdu,
    signed: &str,
    sig: &str,
    cert: &str,
    cc: Option<&str>,
) -> Result<String> {
    let signed = unb64(signed)?;
    let root = find(&signed, 0x30).ok_or_else(|| protocol("Invalid smdpSigned2"))?;
    let tx = find(root, 0x80).ok_or_else(|| protocol("Invalid smdpSigned2"))?;
    let flag = find(root, 1).ok_or_else(|| protocol("Invalid smdpSigned2"))?;
    let mut content = signed.clone();
    content.extend(unb64(sig)?);
    if integer(flag)? != 0 {
        let cc = cc
            .filter(|s| !s.is_empty())
            .ok_or_else(|| protocol("Confirmation code required but not provided"))?;
        let mut hash = Sha256::new();
        hash.update(Sha256::digest(cc.as_bytes()));
        hash.update(tx);
        content.extend(encode(4, &hash.finalize()));
    }
    content.extend(unb64(cert)?);
    let response = command(client, &encode(0xbf21, &content))?;
    require(&response, 0xbf21, "PrepareDownloadResponse")?;
    Ok(b64(&response))
}
pub fn metadata(data: &str) -> Result<Value> {
    let bytes = unb64(data)?;
    crate::profiles::decode(
        find(&bytes, 0xbf25).ok_or_else(|| protocol("Invalid profileMetadata"))?,
    )
}
pub fn cancel(client: &mut impl Apdu, tx: &[u8], reason: u8) -> Result<String> {
    let mut inner = encode(0x80, tx);
    inner.extend(encode(0x81, &[reason]));
    Ok(b64(&command(client, &encode(0xbf41, &inner))?))
}
fn field(data: &Value, key: &str) -> Result<String> {
    Ok(trim_b64(
        data[key]
            .as_str()
            .ok_or_else(|| protocol(format!("Missing {key}")))?,
    ))
}
pub fn download(
    client: &mut impl Apdu,
    http: &mut impl Es9,
    activation_code: &str,
) -> Result<Option<String>> {
    crate::http::require_time()?;
    let (smdp, matching) = activation(activation_code)?;
    let (challenge, info) = challenge_and_info(client)?;
    let mut tx = None;
    let result = (|| {
        let auth=http.request(smdp,"initiateAuthentication",json!({"smdpAddress":smdp,"euiccChallenge":b64(&challenge),"euiccInfo1":b64(&info),"matchingId":matching}),"Authentication")?;
        let id = field(&auth, "transactionId")?;
        tx = Some(id.clone());
        let authenticated = authenticate_server(
            client,
            &field(&auth, "serverSigned1")?,
            &field(&auth, "serverSignature1")?,
            &field(&auth, "euiccCiPKIdToBeUsed")?,
            &field(&auth, "serverCertificate")?,
            matching,
        )?;
        let cli = http.request(
            smdp,
            "authenticateClient",
            json!({"transactionId":id,"authenticateServerResponse":authenticated}),
            "Authentication",
        )?;
        let iccid = metadata(&field(&cli, "profileMetadata")?)?["iccid"]
            .as_str()
            .map(str::to_owned);
        let prepared = prepare(
            client,
            &field(&cli, "smdpSigned2")?,
            &field(&cli, "smdpSignature2")?,
            &field(&cli, "smdpCertificate")?,
            None,
        )?;
        let bpp = http.request(
            smdp,
            "getBoundProfilePackage",
            json!({"transactionId":id,"prepareDownloadResponse":prepared}),
            "GetBoundProfilePackage",
        )?;
        crate::bpp::load_bpp(client, &field(&bpp, "boundProfilePackage")?)?;
        Ok(iccid)
    })();
    if result.is_err() {
        if let Some(id) = tx.filter(|id| !id.is_empty()) {
            let response = unb64(&id)
                .and_then(|tx| cancel(client, &tx, 127))
                .unwrap_or_default();
            let _ = http.request(
                smdp,
                "cancelSession",
                json!({"transactionId":id,"cancelSessionResponse":response}),
                "CancelSession",
            );
        }
    }
    result
}
