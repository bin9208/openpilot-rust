use super::{config::Config, report, status, storage};
use crate::{youtube_live::profiles, Error, Value};

pub(super) fn print(config: &Config, supplied: Option<Value>) -> Result<i32, Error> {
    let status = match supplied {
        Some(value) if value.truth() => value,
        _ => status::get(config)?,
    };
    let report = report::compact(config, &status)?;
    let temporary = config.paths.report.with_extension("tmp");
    crate::state_json::write_json(&temporary, &report)?;
    std::fs::rename(temporary, &config.paths.report)?;
    let began = if status.get("started_mono").truth() {
        status.get("started_mono").float()?
    } else {
        0.0
    };
    let elapsed = if began != 0.0 {
        (storage::monotonic() - began).max(0.0) as u64
    } else {
        0
    };
    let duration = format!(
        "{:02}:{:02}:{:02}",
        elapsed / 3600,
        elapsed % 3600 / 60,
        elapsed % 60
    );
    let quality = if status.get("quality_label").truth() {
        storage::text(status.get("quality_label"))
    } else {
        profiles::selected(i32::try_from(storage::integer(status.get("quality"))?).unwrap_or(0))
            .label
            .into()
    };
    let alive = status.get("runner_alive").truth();
    let pid = storage::integer(status.get("runner_pid"))?;
    println!(
        "[youtube-test] {} elapsed={duration} quality={quality}",
        storage::text(status.get("status"))
    );
    println!(
        "  runner           pid={} alive={alive}",
        if pid != 0 {
            pid.to_string()
        } else {
            "-".into()
        }
    );
    if let Value::Object(children) = status.get("children") {
        for (name, child) in children {
            let pid = storage::integer(child.get("pid"))?;
            println!(
                "  {:<16} pid={} alive={}",
                Value::Text(name.clone()).string()?,
                if pid != 0 {
                    pid.to_string()
                } else {
                    "-".into()
                },
                child.get("alive").truth()
            );
        }
    }
    let streams = match status.get("vipc_streams") {
        Value::Array(items) => items
            .iter()
            .map(|value| Ok(value.py_string()?.string()?))
            .collect::<Result<Vec<_>, Error>>()?
            .join(", "),
        _ => String::new(),
    };
    println!(
        "  VIPC streams     {}",
        if streams.is_empty() { "-" } else { &streams }
    );
    let youtube = status.get("youtube");
    let state = storage::text(youtube.get("state"));
    println!(
        "  YouTube state    {}",
        if state.is_empty() { "-" } else { &state }
    );
    let display = |key: &str| {
        if youtube.get(key).truth() {
            storage::text(youtube.get(key))
        } else {
            "0".into()
        }
    };
    println!(
        "  source           {} fps / {} kbps / keyframes={}",
        display("stream_source_recent_fps"),
        display("stream_source_recent_kbps"),
        display("stream_source_keyframes")
    );
    println!(
        "  upload           {} kbps / drains={} / pending={} B",
        display("upload_recent_kbps"),
        display("rtmp_drain_calls"),
        display("mux_pending_bytes")
    );
    println!(
        "  writer           {}/{} frames / {}/{} B / write={}ms max={}ms",
        display("rtmp_writer_pending_frames"),
        display("rtmp_writer_capacity"),
        display("rtmp_writer_pending_bytes"),
        display("rtmp_writer_capacity_bytes"),
        display("rtmp_writer_last_write_ms"),
        display("rtmp_writer_max_write_ms")
    );
    let diagnosis = report.get("diagnosis");
    println!(
        "  verdict          {}",
        storage::text(diagnosis.get("verdict"))
    );
    for (key, prefix) in [("failures", "fail"), ("warnings", "warning")] {
        if let Value::Array(items) = diagnosis.get(key) {
            for item in items {
                println!("  {prefix:<16} {}", storage::text(item));
            }
        }
    }
    println!(
        "  log              {}",
        storage::text(status.get("log_path"))
    );
    println!("  report           {}", config.paths.report.display());
    let error = storage::text(if status.get("error").truth() {
        status.get("error")
    } else {
        youtube.get("last_error")
    });
    if !error.trim().is_empty() {
        println!("  error            {}", error.trim());
    }
    Ok(if alive { 0 } else { 1 })
}
