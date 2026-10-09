use super::{profiles, settings::Settings};
use crate::Value;
use std::fs;

#[derive(Default)]
pub(super) struct Resources {
    cached: Option<(f64, Value)>,
}
impl Resources {
    pub fn processes(&mut self, mono: f64) -> Value {
        if let Some((at, value)) = &self.cached {
            if mono - at < 5.0 {
                return value.clone();
            }
        }
        let mut names = vec![
            (
                "carrot_cluster",
                b"selfdrive.carrot.cluster_autorun".to_vec(),
            ),
            ("webrtcd", b"system.webrtc.carrot_webrtcd".to_vec()),
            (
                "stream_encoderd",
                b"encoderd\0--carrot-vision-road".to_vec(),
            ),
        ];
        names.extend(profiles::PROFILES.into_iter().map(|profile| {
            (
                profile.process,
                format!("encoderd\0{}\0", profile.encoder_flag).into_bytes(),
            )
        }));
        let mut pids: Vec<Vec<u32>> = names.iter().map(|_| Vec::new()).collect();
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u32>().ok())
                else {
                    continue;
                };
                let Ok(bytes) = fs::read(entry.path().join("cmdline")) else {
                    continue;
                };
                for ((_, pattern), matched) in names.iter().zip(pids.iter_mut()) {
                    if bytes.windows(pattern.len()).any(|part| part == pattern) {
                        matched.push(pid);
                    }
                }
            }
        }
        let value = Value::Object(
            names
                .iter()
                .zip(pids)
                .map(|((name, _), mut pids)| {
                    pids.sort_unstable();
                    (
                        name.chars().map(u32::from).collect(),
                        Value::object([
                            ("running", Value::Bool(!pids.is_empty())),
                            (
                                "pids",
                                Value::Array(
                                    pids.into_iter().take(8).map(Value::integer).collect(),
                                ),
                            ),
                        ]),
                    )
                })
                .collect(),
        );
        self.cached = Some((mono, value.clone()));
        value
    }
    pub fn status(&mut self, settings: &mut Settings, mono: f64) -> Value {
        let processes = self.processes(mono);
        let cluster = settings.integer("ClusterHud", mono);
        let dm = settings.integer("DisableDM", mono);
        let profile = profiles::selected(settings.integer("CarrotYouTubeQuality", mono));
        let cluster_process = processes.get("carrot_cluster");
        let rtc = processes.get("webrtcd");
        let encoder = processes.get("stream_encoderd");
        let selected = processes.get(profile.process);
        Value::object([
            (
                "cluster",
                Value::object([
                    ("enabled", Value::Bool(cluster == 1)),
                    ("configured", Value::Bool(cluster == 1)),
                    ("active", cluster_process.get("running").clone()),
                    ("param", Value::integer(cluster)),
                    ("running", cluster_process.get("running").clone()),
                    ("pids", cluster_process.get("pids").clone()),
                ]),
            ),
            (
                "carrot_vision",
                Value::object([
                    ("enabled", Value::Bool(dm == 2)),
                    ("configured", Value::Bool(dm == 2)),
                    (
                        "active",
                        Value::Bool(rtc.get("running").truth() || encoder.get("running").truth()),
                    ),
                    ("disable_dm", Value::integer(dm)),
                    ("webrtcd_running", rtc.get("running").clone()),
                    ("stream_encoderd_running", encoder.get("running").clone()),
                    ("webrtcd_pids", rtc.get("pids").clone()),
                    ("stream_encoderd_pids", encoder.get("pids").clone()),
                ]),
            ),
            (
                "youtube_encoder",
                Value::object([
                    ("selected", Value::text(profile.process)),
                    ("running", selected.get("running").clone()),
                    ("pids", selected.get("pids").clone()),
                ]),
            ),
        ])
    }
}
