use super::{integer, text, unsigned, Error, Projection};
use crate::{media::request::Request, models::NaviMediaFrame};
use openpilot_carrot_navi::{manifest::CATALOG, receiver::Dashboard};
use std::sync::Arc;

impl Projection {
    pub(super) fn update_media(&mut self, snapshot: &Dashboard) -> Result<(), Error> {
        for &(kind, name) in CATALOG {
            if kind == "json" {
                continue;
            }
            let key = format!("{kind}:{name}");
            let Some((_, record)) = snapshot.records.iter().find(|(name, _)| name == &key) else {
                continue;
            };
            let summary = record.summary();
            let sequence = unsigned(summary.get("sequence"))?;
            if !summary.get("present").truth() {
                self.requested.insert(key.clone(), sequence);
                self.worker.discard(&key);
                self.media.insert(
                    key.clone(),
                    Arc::new(NaviMediaFrame {
                        key,
                        sequence,
                        present: false,
                        reason: text(summary.get("reason"))?,
                        ..NaviMediaFrame::default()
                    }),
                );
                continue;
            }
            if self.requested.get(&key) == Some(&sequence) {
                continue;
            }
            let kind = integer(summary.get("message_type"))?;
            let payload = record.payload().unwrap_or_default();
            if kind == 1 {
                self.requested.insert(key.clone(), sequence);
                self.worker.discard(&key);
                self.media.insert(
                    key.clone(),
                    Arc::new(NaviMediaFrame {
                        key,
                        sequence,
                        present: true,
                        mime: if integer(summary.get("format_or_reason"))? == 1 {
                            "image/png"
                        } else {
                            "image/jpeg"
                        }
                        .into(),
                        width: integer(summary.get("width"))?,
                        height: integer(summary.get("height"))?,
                        data: Some(Arc::from(payload)),
                        ..NaviMediaFrame::default()
                    }),
                );
            } else if kind == 3 && !payload.is_empty() {
                self.requested.insert(key.clone(), sequence);
                let config = snapshot
                    .binary_configs
                    .iter()
                    .find(|(name, _)| name == &key)
                    .and_then(|(_, record)| record.payload())
                    .unwrap_or_default();
                self.worker.submit(Request {
                    epoch: self.epoch,
                    key,
                    sequence,
                    payload: payload.to_vec(),
                    config: config.to_vec(),
                    keyframe: unsigned(summary.get("flags"))? & 1 != 0,
                    width: integer(summary.get("width"))?,
                    height: integer(summary.get("height"))?,
                    reset: false,
                });
            }
        }
        Ok(())
    }
    pub(super) fn apply_results(&mut self) {
        for result in self.worker.poll() {
            if result.epoch != self.epoch
                || self
                    .media
                    .get(&result.frame.key)
                    .is_some_and(|current| result.frame.sequence <= current.sequence)
            {
                continue;
            }
            self.media.insert(result.frame.key.clone(), result.frame);
        }
    }
}
