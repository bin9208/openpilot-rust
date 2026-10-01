use openpilot_athena::{policy, queue::Uploads, Error};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
    path::Path,
};

fn trace(input: Value) -> Result<Value, Error> {
    let root = Path::new(input["root"].as_str().ok_or(Error::Contract("root"))?);
    let rows = input["operations"]
        .as_array()
        .ok_or(Error::Contract("operations"))?;
    let mut uploads = Uploads::default();
    let mut params = BTreeMap::<String, Value>::new();
    let mut records = Vec::new();
    for row in rows {
        let args = row
            .get("args")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let op = row["op"].as_str().ok_or(Error::Contract("op"))?;
        let result = match op {
            "params" => {
                params.extend(serde_json::from_value::<BTreeMap<String, Value>>(
                    row["values"].clone(),
                )?);
                Value::Null
            }
            "initialize" => {
                if let Some(value) = params.get("AthenadUploadQueue") {
                    if let Err(error) = uploads.initialize(value.to_string().as_bytes()) {
                        eprintln!("source-compatible cache parse failure: {error}");
                    }
                }
                Value::Null
            }
            "pop" => serde_json::to_value(uploads.queued.pop())?,
            "current" => {
                uploads.set_current(
                    usize::try_from(row["tid"].as_u64().ok_or(Error::Contract("tid"))?)
                        .map_err(|_| Error::Contract("tid range"))?,
                    Some(serde_json::from_value(row["item"].clone())?),
                );
                Value::Null
            }
            "cache" => {
                params.insert(
                    "AthenadUploadQueue".into(),
                    serde_json::from_slice(&uploads.cache()?)?,
                );
                Value::Null
            }
            "retry" => {
                let tid = usize::try_from(row["tid"].as_u64().ok_or(Error::Contract("tid"))?)
                    .map_err(|_| Error::Contract("tid range"))?;
                let current = uploads
                    .current
                    .iter()
                    .find(|(id, _)| *id == tid)
                    .and_then(|(_, item)| item.clone());
                if let Some(item) =
                    current.and_then(|item| item.retry(row["increase"].as_bool().unwrap_or(true)))
                {
                    uploads.queued.push(item);
                    params.insert(
                        "AthenadUploadQueue".into(),
                        serde_json::from_slice(&uploads.cache()?)?,
                    );
                    uploads.set_current(tid, None);
                }
                Value::Null
            }
            "cancelFirst" => {
                let id = uploads
                    .list()
                    .first()
                    .ok_or(Error::Contract("empty queue"))?
                    .id
                    .clone();
                uploads.cancel(&[id])
            }
            "uploadFilesToUrls" | "uploadFileToUrl" => {
                let files = if op == "uploadFilesToUrls" {
                    args[0]
                        .as_array()
                        .cloned()
                        .ok_or(Error::Contract("files"))?
                } else {
                    vec![json!({"fn":args[0],"url":args[1],"headers":args[2]})]
                };
                let result = uploads.enqueue(root, &files, 1720000000125)?;
                params.insert(
                    "AthenadUploadQueue".into(),
                    serde_json::from_slice(&uploads.cache()?)?,
                );
                result
            }
            "listUploadQueue" => serde_json::to_value(uploads.list())?,
            "cancelUpload" => {
                let ids = match &args[0] {
                    Value::Array(values) => values.clone(),
                    value => vec![value.clone()],
                };
                uploads.cancel(
                    &ids.into_iter()
                        .map(serde_json::from_value)
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }
            "setRouteViewed" => {
                let previous = params
                    .get("AthenadRecentlyViewedRoutes")
                    .and_then(Value::as_str);
                let value = policy::viewed_routes(
                    previous,
                    args[0].as_str().ok_or(Error::Contract("route"))?,
                );
                params.insert("AthenadRecentlyViewedRoutes".into(), value.into());
                json!({"success":1})
            }
            "strip_zst_extension" => {
                policy::strip_zst(args[0].as_str().ok_or(Error::Contract("filename"))?).into()
            }
            _ => return Err(Error::Contract("unknown trace operation")),
        };
        records.push(json!({"op":op,"result":result,"queue":uploads.list(),"params":params}));
    }
    Ok(records.into())
}
fn main() -> Result<(), Error> {
    for line in io::stdin().lock().lines() {
        println!("{}", trace(serde_json::from_str(&line?)?)?);
    }
    Ok(())
}
