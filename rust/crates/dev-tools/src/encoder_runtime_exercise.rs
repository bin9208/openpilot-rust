//! Original frame schedules and lifecycle assertions, shared by real IPC and tests.
use crate::seal::Result;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub trait Driver {
    fn send(&mut self, frame: u32, pause: f64) -> Result<()>;
    fn await_frame(&mut self, frame: u32) -> Result<()>;
    fn set_active(&mut self, active: bool) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn restart(&mut self) -> Result<()>;
    fn drain(&mut self) -> Result<()>;
    fn rows(&self) -> &BTreeMap<String, Vec<Value>>;
}
pub fn services(case: &str) -> Vec<&'static str> {
    if case.starts_with("main") {
        let mut services = vec!["roadEncodeData", "qRoadEncodeData", "thumbnail"];
        if case == "main-multi" {
            services.extend(["driverEncodeData", "wideRoadEncodeData"]);
        }
        services
    } else if ["--stream", "--carrot-vision-road"].contains(&case) {
        vec!["livestreamRoadEncodeData"]
    } else {
        vec!["youtubeRoadEncodeData"]
    }
}
fn counts(peer: &dyn Driver) -> BTreeMap<String, usize> {
    peer.rows()
        .iter()
        .map(|(s, v)| (s.clone(), v.len()))
        .collect()
}
pub fn exercise(case: &str, peer: &mut dyn Driver) -> Result<()> {
    if case == "main-multi" {
        peer.send(98, 0.2)?;
        peer.send(99, 0.2)?;
    }
    if case == "main-lag" {
        peer.stop()?;
        let sent = (|| -> Result<()> {
            for f in 100..170 {
                peer.send(f, 0.)?;
            }
            Ok(())
        })();
        peer.resume()?;
        sent?;
        peer.await_frame(169)?;
        for f in 170..240 {
            peer.send(f, 0.05)?;
        }
        peer.await_frame(239)?;
        if peer
            .rows()
            .get("roadEncodeData")
            .and_then(|r| r.first())
            .and_then(|v| v["idx"]["frameId"].as_u64())
            != Some(106)
        {
            return Err("overwritten native buffers were not dropped".into());
        }
        if !peer.rows()["thumbnail"].is_empty() {
            return Err("overwritten thumbnail frame was encoded".into());
        }
    } else if case == "--carrot-vision-road" {
        for f in 100..121 {
            peer.send(f, 0.05)?;
        }
        if peer.rows().values().any(|v| !v.is_empty()) {
            return Err("inactive software prewarm unexpectedly produced a packet".into());
        }
        peer.set_active(true)?;
        for f in 121..206 {
            peer.send(f, 0.05)?;
        }
        peer.await_frame(205)?;
        let previous = counts(peer);
        peer.set_active(false)?;
        for f in 206..216 {
            peer.send(f, 0.05)?;
        }
        if counts(peer) != previous {
            return Err("inactive encoder published new frames".into());
        }
        peer.set_active(true)?;
        for f in 216..266 {
            peer.send(f, 0.05)?;
        }
        peer.await_frame(265)?;
        if peer
            .rows()
            .values()
            .flat_map(|v| v.iter())
            .any(|r| r["idx"]["segmentNum"].as_i64() != Some(0))
        {
            return Err("on-demand encoder rotated".into());
        }
    } else {
        for f in 100..230 {
            peer.send(f, 0.05)?;
        }
        peer.await_frame(229)?;
        if case.starts_with("main") && peer.rows()["thumbnail"].len() != 1 {
            return Err("missing frame-100 thumbnail".into());
        }
        for service in services(case) {
            let rows = peer.rows().get(service).ok_or("missing encoder service")?;
            if service != "thumbnail"
                && rows
                    .iter()
                    .map(|r| r["idx"]["segmentNum"].as_i64())
                    .collect::<BTreeSet<_>>()
                    != [Some(0), Some(1)].into()
            {
                return Err(format!("missing rotated packets: {service}").into());
            }
        }
    }
    if case == "main-restart" {
        let previous = counts(peer);
        peer.restart()?;
        for f in 500..520 {
            peer.send(f, 0.05)?;
        }
        peer.drain()?;
        if previous != counts(peer) {
            return Err("encoder changed inherited receive-only restart behavior".into());
        }
    }
    Ok(())
}
