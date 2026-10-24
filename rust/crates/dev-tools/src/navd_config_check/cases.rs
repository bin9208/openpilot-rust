//! Source-derived inputs; no captured outputs or source file execution.
use super::{values as v, Case};
use crate::seal::Result;
use serde_json::{json, Value};
pub(super) fn cases() -> Result<Vec<Case>> {
    let bytes = |s: &[u8]| json!({"bytes_hex":v::hex(s)});
    let mut rows = vec![
        json!({"name":"environment","environment":"override-token","params":{"PrimeType":bytes(b"bad")}}),
        json!({"name":"environment-empty","environment":"","params":{"PrimeType":bytes(b"1")}}),
        json!({"name":"public-key","params":{"PrimeType":bytes(b"0"),"MapboxPublicKey":bytes(b"public-token")}}),
        json!({"name":"missing-type","params":{"MapboxPublicKey":bytes(b"public-token")}}),
        json!({"name":"public-key-empty","params":{"MapboxPublicKey":bytes(b"")}}),
        json!({"name":"public-key-invalid-utf8","params":{"MapboxPublicKey":bytes(b"\xff")}}),
        json!({"name":"public-key-missing","params":{}}),
    ];
    for (name, extra) in [
        (
            "signed-rsa",
            json!({"params":{"PrimeType":bytes(b"1"),"DongleId":bytes(b"fixture-dongle")},"keys":["rsa"]}),
        ),
        (
            "signed-ec",
            json!({"params":{"PrimeType":bytes(b"2"),"DongleId":bytes(b"fixture-dongle")},"keys":["ec"]}),
        ),
        (
            "rsa-precedence",
            json!({"params":{"PrimeType":bytes(b"1"),"DongleId":bytes(b"fixture-dongle")},"keys":["rsa","ec"]}),
        ),
        (
            "signed-null-identity",
            json!({"params":{"PrimeType":bytes(b"1")},"keys":["rsa"]}),
        ),
        (
            "signed-invalid-identity",
            json!({"params":{"PrimeType":bytes(b"1"),"DongleId":bytes(b"\xff")},"keys":["rsa"]}),
        ),
        (
            "signed-missing-key",
            json!({"params":{"PrimeType":bytes(b"1")}}),
        ),
        (
            "missing-key-unicode",
            json!({"params":{"PrimeType":bytes(b"1"),"DongleId":bytes("기기\x7f😀".as_bytes())}}),
        ),
        (
            "signed-missing-public",
            json!({"params":{"PrimeType":bytes(b"1")},"keys":["rsa"],"no_public":true}),
        ),
        (
            "signed-invalid-private",
            json!({"params":{"PrimeType":bytes(b"1")},"keys":["rsa"],"bad_private":true}),
        ),
    ] {
        let mut row = extra.as_object().cloned().ok_or("case object")?;
        row.insert("name".into(), Value::String(name.into()));
        rows.push(Value::Object(row));
    }
    for (i, raw) in [
        b"".as_slice(),
        b" +0 trailing",
        b"-0",
        b"1junk",
        b"-1",
        b"2147483647",
        b"-2147483648",
        b"bad",
        b"2147483648",
        b"-2147483649",
    ]
    .iter()
    .enumerate()
    {
        rows.push(json!({"name":format!("integer-{i}"),"params":{"PrimeType":bytes(raw),"MapboxPublicKey":bytes(b"public")},"keys":["rsa"]}));
    }
    rows.into_iter()
        .map(|row| {
            Ok(Case {
                name: row["name"].as_str().ok_or("case name")?.into(),
                input: v::value(row)?,
            })
        })
        .collect()
}
