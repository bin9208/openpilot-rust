use crate::{
    heap::Queue,
    policy::{strip_zst, UploadItem},
    Error,
};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};

#[derive(Default)]
pub struct Uploads {
    pub queued: Queue,
    pub current: Vec<(usize, Option<UploadItem>)>,
    pub cancelled: HashSet<Option<String>>,
}
impl Uploads {
    pub fn list(&self) -> Vec<&UploadItem> {
        self.queued
            .0
            .iter()
            .chain(self.current.iter().filter_map(|(_, item)| item.as_ref()))
            .filter(|item| !self.cancelled.contains(&item.id))
            .collect()
    }

    pub fn cache(&self) -> Result<Vec<u8>, Error> {
        Ok(serde_json::to_vec(
            &self
                .queued
                .0
                .iter()
                .filter(|item| !self.cancelled.contains(&item.id))
                .collect::<Vec<_>>(),
        )?)
    }

    pub fn initialize(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let items: Vec<Value> = serde_json::from_slice(bytes)?;
        for item in items {
            self.queued.push(serde_json::from_value(item)?);
        }
        Ok(())
    }

    pub fn set_current(&mut self, worker: usize, item: Option<UploadItem>) {
        if let Some((_, current)) = self.current.iter_mut().find(|(id, _)| *id == worker) {
            *current = item;
        } else {
            self.current.push((worker, item));
        }
    }

    pub fn cancel(&mut self, ids: &[Option<String>]) -> Value {
        let cancelled: Vec<_> = self
            .queued
            .0
            .iter()
            .filter(|item| ids.contains(&item.id))
            .map(|item| item.id.clone())
            .collect();
        if cancelled.is_empty() {
            json!({"success":0,"error":"not found"})
        } else {
            self.cancelled.extend(cancelled);
            json!({"success":1})
        }
    }

    pub fn enqueue(&mut self, root: &Path, files: &[Value], now_ms: i64) -> Result<Value, Error> {
        let mut added = Vec::new();
        let mut failed = Vec::new();
        for file in files {
            let name = text_field(file, "fn", "")?;
            let url = text_field(file, "url", "")?;
            if name.is_empty() || name.starts_with('/') || name.contains("..") || url.is_empty() {
                failed.push(name);
                continue;
            }
            let path = root.join(&name);
            let path = path.to_str().ok_or(Error::Contract("unencodable path"))?;
            if !Path::new(path).exists() && !Path::new(strip_zst(path)).exists() {
                failed.push(name);
                continue;
            }
            if self
                .list()
                .iter()
                .any(|item| item.url.split('?').next() == url.split('?').next())
            {
                continue;
            }
            let headers = file.get("headers").cloned().unwrap_or_else(|| json!({}));
            let mut item = UploadItem {
                path: path.into(),
                url,
                headers: serde_json::from_value(headers)?,
                created_at: now_ms,
                id: None,
                retry_count: 0,
                current: false,
                progress: 0.,
                allow_cellular: file
                    .get("allow_cellular")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                priority: file.get("priority").and_then(Value::as_i64).unwrap_or(99),
            };
            item.assign_id()?;
            added.push(item.clone());
            self.queued.push(item);
        }
        let mut result = json!({"enqueued":added.len(), "items":added});
        if !failed.is_empty() {
            result["failed"] = failed.into();
        }
        Ok(result)
    }
}

fn text_field(file: &Value, key: &str, default: &str) -> Result<String, Error> {
    match file.get(key) {
        None => Ok(default.into()),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(Error::Contract("upload file field requires text")),
    }
}
