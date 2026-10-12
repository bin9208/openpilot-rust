use openpilot_cluster::route_replay::video::{
    path_for_stream, segment_at, Reader, Segment, VideoConfig,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Query {
    segments: Vec<Segment>,
    stream: String,
    config: VideoConfig,
    #[serde(default)]
    steps: Vec<Step>,
    output: PathBuf,
}
#[derive(Deserialize)]
struct Step {
    time: Option<f64>,
    segments: Option<Vec<Segment>>,
    #[serde(default)]
    close: bool,
}
#[derive(Serialize)]
struct Decoded {
    width: i32,
    height: i32,
    frame_id: String,
}
#[derive(Serialize)]
struct Snapshot {
    status: String,
    process: bool,
    frame: Option<Decoded>,
}
fn step(
    reader: &mut Reader,
    input: Step,
    path: &std::path::Path,
) -> Result<Snapshot, Box<dyn std::error::Error>> {
    if let Some(segments) = input.segments {
        reader.set_segments(segments);
    }
    let frame = if input.close {
        reader.close()?;
        None
    } else {
        reader.frame_at(input.time.ok_or("missing time")?)?
    };
    let frame = match frame {
        Some(frame) => {
            std::fs::write(path, &frame.rgba)?;
            Some(Decoded {
                width: frame.width,
                height: frame.height,
                frame_id: frame.frame_id,
            })
        }
        None => None,
    };
    Ok(Snapshot {
        status: reader.status_text(),
        process: reader.process_id().is_some(),
        frame,
    })
}
#[derive(Deserialize)]
struct Helpers {
    segments: Vec<Segment>,
    times: Vec<f64>,
    folder: PathBuf,
    streams: Vec<String>,
    config: VideoConfig,
    #[serde(default)]
    bundled: Option<PathBuf>,
}
fn helpers() -> Result<(), Box<dyn std::error::Error>> {
    let query: Helpers = serde_json::from_reader(std::io::stdin().lock())?;
    let selections: Vec<_> = query
        .times
        .iter()
        .map(|&time| segment_at(&query.segments, time))
        .collect();
    let paths: Vec<_> = query
        .streams
        .iter()
        .map(|s| path_for_stream(&query.folder, s))
        .collect();
    let discovered = VideoConfig::discover(query.bundled.as_deref(), query.config.cwd)?.ffmpeg;
    println!(
        "{}",
        serde_json::to_string(
            &serde_json::json!({"segments":selections,"paths":paths,"discovered":discovered})
        )?
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1);
    if mode.as_deref() == Some("helpers") {
        return helpers();
    }
    if mode.as_deref() == Some("live") {
        let mut input = std::io::stdin().lock().lines();
        let query: Query = serde_json::from_str(&input.next().ok_or("missing query")??)?;
        std::fs::create_dir_all(&query.output)?;
        let mut reader = Reader::new(query.segments, &query.stream, query.config);
        for (index, line) in input.enumerate() {
            let snapshot = step(
                &mut reader,
                serde_json::from_str(&line?)?,
                &query.output.join(format!("{index}.rgba")),
            )?;
            println!(
                "{}",
                serde_json::to_string(
                    &serde_json::json!({"snapshot":snapshot,"pid":reader.process_id()})
                )?
            );
            std::io::stdout().flush()?;
        }
        reader.close()?;
        return Ok(());
    }
    let query: Query = serde_json::from_reader(std::io::stdin().lock())?;
    std::fs::create_dir_all(&query.output)?;
    let mut reader = Reader::new(query.segments, &query.stream, query.config);
    let mut steps = Vec::new();
    let mut pids = Vec::new();
    for (index, input) in query.steps.into_iter().enumerate() {
        steps.push(step(
            &mut reader,
            input,
            &query.output.join(format!("{index}.rgba")),
        )?);
        if let Some(pid) = reader.process_id() {
            if !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }
    reader.close()?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({"steps":steps,"pids":pids}))?
    );
    Ok(())
}
