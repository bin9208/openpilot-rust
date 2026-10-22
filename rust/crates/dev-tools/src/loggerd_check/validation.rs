//! Full Event comparison and source-declared metadata/media projections only.
use super::{hash, hex, peer::Evidence as Peer, require, save, Result};
use crate::qa_process::{CommandSpec, NativeRunner};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn bytes(value: Option<&Value>) -> Result<Vec<u8>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let text = value
        .get("hex")
        .and_then(Value::as_str)
        .ok_or("schema Data is not a byte-preserving hex object")?;
    require(text.len().is_multiple_of(2), "odd schema Data encoding")?;
    (0..text.len())
        .step_by(2)
        .map(|n| u8::from_str_radix(&text[n..n + 2], 16).map_err(Into::into))
        .collect()
}
fn entries(value: &Value) -> Result<BTreeMap<String, Vec<u8>>> {
    value["entries"]
        .as_array()
        .ok_or("schema map entries missing")?
        .iter()
        .map(|row| {
            Ok((
                row["key"]
                    .as_str()
                    .ok_or("schema map key missing")?
                    .to_owned(),
                bytes(row.get("value"))?,
            ))
        })
        .collect()
}
pub(super) fn normalize(records: &Value) -> Result<Value> {
    let mut result = Vec::new();
    for original in records.as_array().ok_or("logger record list missing")? {
        let mut row = original.clone();
        if row["logMessage"] == "logger-qa-barrier" {
            continue;
        }
        if row.get("initData").is_some() {
            row["logMonoTime"] = json!(0);
            row["initData"]["wallTimeNanos"] = json!(0);
            let entries = row["initData"]["commands"]["entries"]
                .as_array_mut()
                .ok_or("init command list absent")?;
            entries.retain(|entry| {
                !matches!(
                    entry["key"].as_str(),
                    Some(
                        "loggerd implementation" | "loggerd source commit" | "loggerd source tree"
                    )
                )
            });
            for entry in entries {
                if entry["key"] == "df -h" {
                    entry["value"] = json!({"hex":hex(b"<ambient filesystem usage>")});
                }
            }
        }
        if row.get("sentinel").is_some() {
            row["logMonoTime"] = json!(0);
        }
        result.push(row);
    }
    Ok(json!(result))
}
fn metadata(records: &Value, native: bool) -> Result<()> {
    let init = &records
        .as_array()
        .and_then(|r| r.first())
        .ok_or("logger init record absent")?["initData"];
    require(
        init["dongleId"] == "host-qa-fixture"
            && init["gitCommit"] == "1111111111111111111111111111111111111111"
            && init["gitBranch"] == "host-qa",
        "logger fixture metadata differs",
    )?;
    let params = entries(&init["params"])?;
    require(
        params.get("AccessToken") == Some(&Vec::new())
            && params.get("RouteCount").is_some_and(|v| v == b"43")
            && params
                .get("DongleId")
                .is_some_and(|v| v == b"host-qa-fixture"),
        "logger Params redaction/counter metadata differs",
    )?;
    if native {
        let commands = entries(&init["commands"])?;
        require(
            commands
                .get("loggerd implementation")
                .is_some_and(|v| v == b"openpilot-loggerd Rust"),
            "native logger implementation metadata missing",
        )?;
        let commit = commands
            .get("loggerd source commit")
            .ok_or("native logger source commit missing")?;
        require(
            commit.len() == 40
                && commit.iter().all(u8::is_ascii_hexdigit)
                && commit.iter().any(|v| *v != b'0'),
            "native logger source commit invalid",
        )?;
        require(
            commands.get("loggerd source tree").is_some_and(|v| {
                [b"clean".as_slice(), b"dirty", b"unknown"].contains(&v.as_slice())
            }),
            "native logger source tree invalid",
        )?;
    }
    Ok(())
}
fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return haystack.len() + 1;
    }
    let mut count = 0;
    let mut at = 0;
    while at + needle.len() <= haystack.len() {
        if &haystack[at..at + needle.len()] == needle {
            count += 1;
            at += needle.len();
        } else {
            at += 1;
        }
    }
    count
}
fn decimation(peer: &Peer, logs: &[Value]) -> Result<Value> {
    let mut expected = [BTreeMap::<Vec<u8>, usize>::new(), BTreeMap::new()];
    let mut counters = BTreeMap::<String, usize>::new();
    for (service, packet) in &peer.inputs {
        if service.ends_with("EncodeData") {
            continue;
        }
        *expected[0].entry(packet.clone()).or_default() += 1;
        let count = counters.entry(service.clone()).or_default();
        let frequency = openpilot_messaging::services::lookup(service)
            .ok_or("decimation service missing")?
            .decimation;
        if let Some(frequency) = frequency {
            require(frequency > 0, "decimation frequency is zero")?;
            if (*count).is_multiple_of(frequency as usize) {
                *expected[1].entry(packet.clone()).or_default() += 1;
            }
        }
        *count += 1;
    }
    for (which, name) in ["rlog", "qlog"].iter().enumerate() {
        let mut content = Vec::new();
        for index in 0..logs.len() {
            content.extend(fs::read(peer.segment(index).join(format!("{name}.capnp")))?);
        }
        for (packet, count) in &expected[which] {
            require(
                occurrences(&content, packet) == *count,
                "logger ordinary/decimated payload count differs",
            )?;
        }
        if which == 1 {
            for packet in expected[0]
                .keys()
                .filter(|packet| !expected[1].contains_key(*packet))
            {
                require(
                    occurrences(&content, packet) == 0,
                    "unexpected qlog payload",
                )?;
            }
        }
    }
    Ok(
        json!({"rlog_ordinary_packets":expected[0].values().sum::<usize>(),"qlog_ordinary_packets":expected[1].values().sum::<usize>(),"ordinary_services":counters}),
    )
}
fn capture(argv: Vec<String>, cancelled: &AtomicBool) -> Result<Vec<u8>> {
    let result = NativeRunner.run_controlled(
        &CommandSpec::new(argv, Duration::from_secs(60)),
        Instant::now() + Duration::from_secs(60),
        cancelled,
    )?;
    require(
        result.code == 0,
        format!(
            "media provider failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    Ok(result.stdout)
}
fn media(path: &Path, cancelled: &AtomicBool) -> Result<Value> {
    let path_text = path.to_str().ok_or("media path is not UTF-8")?;
    let probe = capture(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_streams",
            "-show_packets",
            "-of",
            "json",
            path_text,
        ]
        .map(str::to_owned)
        .to_vec(),
        cancelled,
    )?;
    fs::write(
        path.with_file_name(format!(
            "{}.probe.json",
            path.file_name()
                .ok_or("media name absent")?
                .to_string_lossy()
        )),
        &probe,
    )?;
    let decoded: Value = serde_json::from_slice(&probe)?;
    fn project(rows: &Value, fields: &[&str]) -> Result<Vec<Value>> {
        rows.as_array()
            .ok_or("media provider list missing")?
            .iter()
            .map(|row| {
                let mut selected = serde_json::Map::new();
                for field in fields {
                    if let Some(value) = row.get(*field) {
                        selected.insert((*field).into(), value.clone());
                    }
                }
                Ok(Value::Object(selected))
            })
            .collect()
    }
    let streams = project(
        &decoded["streams"],
        &[
            "codec_name",
            "codec_type",
            "width",
            "height",
            "time_base",
            "sample_rate",
            "channels",
        ],
    )?;
    let packets = project(
        &decoded["packets"],
        &["stream_index", "pts", "dts", "duration", "size", "flags"],
    )?;
    let video = capture(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-i",
            path_text,
            "-map",
            "0:v:0",
            "-fps_mode",
            "passthrough",
            "-f",
            "framemd5",
            "-",
        ]
        .map(str::to_owned)
        .to_vec(),
        cancelled,
    )?;
    fs::write(
        path.with_file_name(format!(
            "{}.frames.md5",
            path.file_name()
                .ok_or("media name absent")?
                .to_string_lossy()
        )),
        &video,
    )?;
    let frames = std::str::from_utf8(&video)?
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(|s| s.trim().to_owned())
        .collect::<Vec<_>>();
    require(!frames.is_empty(), "media decoder produced no video frames")?;
    let mut result = json!({"streams":streams,"packets":packets,"frames":frames});
    if streams.iter().any(|stream| stream["codec_type"] == "audio") {
        let audio = capture(
            [
                "ffmpeg", "-nostdin", "-v", "error", "-i", path_text, "-map", "0:a:0", "-f",
                "f32le", "-",
            ]
            .map(str::to_owned)
            .to_vec(),
            cancelled,
        )?;
        require(!audio.is_empty(), "media decoder produced no audio samples")?;
        fs::write(
            path.with_file_name(format!(
                "{}.audio.f32",
                path.file_name()
                    .ok_or("media name absent")?
                    .to_string_lossy()
            )),
            &audio,
        )?;
        result["audio_bytes"] = json!(audio.len());
        result["audio_sha256"] = json!(hash(&audio));
    }
    Ok(result)
}
pub(super) fn compare(
    source: (&Peer, &[Value]),
    native: (&Peer, &[Value]),
    cancelled: &AtomicBool,
) -> Result<Value> {
    require(
        source.1.len() == native.1.len(),
        "logger segment counts differ",
    )?;
    let mut result = json!({"segments":native.1.len(),"original":decimation(source.0,source.1)?,"rust":decimation(native.0,native.1)?,"media":[]});
    for index in 0..source.1.len() {
        for name in ["rlog", "qlog"] {
            metadata(&source.1[index][name], false)?;
            metadata(&native.1[index][name], true)?;
            let original = normalize(&source.1[index][name])?;
            let actual = normalize(&native.1[index][name])?;
            if original != actual {
                save(
                    &native.0.root.join(format!("{index}-{name}-mismatch.json")),
                    &json!({"expected":original,"actual":actual}),
                )?;
                return Err("full-schema logger records differ".into());
            }
        }
        let files = |row: &Value| -> Result<Vec<String>> {
            Ok(row["files"]
                .as_array()
                .ok_or("logger file list absent")?
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| s.ends_with(".hevc") || s.ends_with(".ts"))
                .map(str::to_owned)
                .collect())
        };
        let names = files(&source.1[index])?;
        require(
            names == files(&native.1[index])?,
            "logger media file names differ",
        )?;
        for name in names {
            let original = media(&source.0.segment(index).join(&name), cancelled)?;
            let actual = media(&native.0.segment(index).join(&name), cancelled)?;
            require(
                original == actual,
                "media streams/packet metadata/decoded video or audio bytes differ",
            )?;
            result["media"].as_array_mut().ok_or("media evidence list absent")?.push(json!({"segment":index,"file":name,"frames":actual["frames"].as_array().ok_or("media frames absent")?.len(),"streams":actual["streams"],"audio_bytes":actual.get("audio_bytes").cloned().unwrap_or(json!(0))}));
        }
    }
    Ok(result)
}
pub(super) fn diagnostic_records(
    root: &Path,
    peer: &Peer,
    native: bool,
) -> Result<(Vec<Value>, Value)> {
    let rows: Vec<Value> = serde_json::from_slice(&fs::read(peer.root.join("diagnostics.json"))?)?;
    require(!rows.is_empty(), "logger diagnostics absent")?;
    let context = rows[0]["ctx"].clone();
    let mut messages = Vec::new();
    for row in rows {
        let object = row
            .as_object()
            .ok_or("logger diagnostic is not an object")?;
        require(
            object.len() == 7
                && [
                    "created", "ctx", "filename", "funcname", "levelnum", "lineno", "msg",
                ]
                .iter()
                .all(|key| object.contains_key(*key)),
            "logger diagnostic fields differ",
        )?;
        require(
            row["ctx"] == context && row["created"].as_f64().is_some_and(|v| v > 0.0),
            "logger diagnostic context/time invalid",
        )?;
        require(
            row["funcname"].as_str().is_some_and(|v| !v.is_empty()),
            "logger diagnostic function absent",
        )?;
        let filename = Path::new(
            row["filename"]
                .as_str()
                .ok_or("logger diagnostic source absent")?,
        );
        let filename = if filename.is_absolute() {
            filename.into()
        } else {
            root.join("rust").join(filename)
        };
        let line = row["lineno"]
            .as_u64()
            .ok_or("logger diagnostic line absent")?;
        require(
            line > 0 && line <= fs::read_to_string(filename)?.lines().count() as u64,
            "logger diagnostic line outside actual source",
        )?;
        let text = row["msg"]
            .as_str()
            .ok_or("logger diagnostic message absent")?
            .replace(peer.root.to_str().ok_or("logger root not UTF-8")?, "<root>")
            .replace(&peer.route, "<route>");
        messages.push(json!([row["levelnum"], text]));
    }
    if native {
        require(
            context["runtime_language"] == "rust"
                && context["source_commit"]
                    .as_str()
                    .is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                && ["clean", "dirty"].contains(&context["source_tree"].as_str().unwrap_or("")),
            "logger diagnostic native provenance differs",
        )?;
    }
    Ok((messages, context))
}
pub(super) fn diagnostics(root: &Path, source: &Peer, native: &Peer) -> Result<Value> {
    let original = diagnostic_records(root, source, false)?;
    let mut actual = diagnostic_records(root, native, true)?;
    let context = actual
        .1
        .as_object_mut()
        .ok_or("native diagnostic context not an object")?;
    for key in ["runtime_language", "source_commit", "source_tree"] {
        context.remove(key);
    }
    require(
        original.0 == actual.0 && original.1 == actual.1,
        "ordered logger diagnostics/context differ",
    )?;
    Ok(
        json!({"records_per_process":original.0.len(),"ordered_levels_and_messages":"exact","context":"exact except verified Rust provenance","callsite":"actual source file, line and function"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_preserves_nonbarrier_payload_timestamps_and_unrelated_metadata() {
        let input = json!([{"logMonoTime":99,"logMessage":"logger-qa-barrier"},{"logMonoTime":123,"can":[{"address":7,"dat":{"hex":"0102"}}]}, {"logMonoTime":77,"initData":{"wallTimeNanos":888,"params":{"entries":[{"key":"must-stay","value":{"hex":"fe"}}]},"commands":{"entries":[{"key":"df -h","value":{"hex":"abcd"}},{"key":"must-stay","value":{"hex":"beef"}},{"key":"loggerd implementation","value":{"hex":"00"}}]}}}]);
        let actual = normalize(&input).unwrap();
        assert_eq!(actual[0], input[1]);
        assert_eq!(
            actual[1]["initData"]["params"],
            input[2]["initData"]["params"]
        );
        assert_eq!(
            actual[1]["initData"]["commands"]["entries"][1],
            input[2]["initData"]["commands"]["entries"][1]
        );
        assert_eq!(actual[1]["logMonoTime"], 0);
        assert_eq!(actual[1]["initData"]["wallTimeNanos"], 0);
        assert_eq!(
            actual[1]["initData"]["commands"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn occurrences_match_source_nonoverlapping_packet_count() {
        assert_eq!(occurrences(b"aaaa", b"aa"), 2);
        assert_eq!(occurrences(b"aaaa", b"aaa"), 1);
        assert_eq!(occurrences(b"data", b"absent"), 0);
    }
}
