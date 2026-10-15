//! Offline commit-time floor before model delivery; never move a current clock back.
use super::Paths;
use crate::Error;
use std::{
    ffi::OsString,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub trait Platform {
    fn run(&mut self, argv: Vec<OsString>) -> Result<Vec<u8>, Error>;
    fn now(&mut self) -> Result<f64, Error>;
    fn message(&mut self, text: &str);
}
fn utc(epoch: f64) -> Result<String, Error> {
    if !epoch.is_finite() || !(-62135596800.0..253402300800.0).contains(&epoch) {
        return Err(Error::Configuration(
            "clock timestamp is outside supported UTC range".into(),
        ));
    }
    let seconds = epoch.trunc() as i64;
    let fractional = (epoch.fract() * 1_000_000.0).round_ties_even() as i64;
    let micros = seconds * 1_000_000 + fractional;
    let time = chrono::DateTime::from_timestamp_micros(micros)
        .ok_or_else(|| Error::Configuration("invalid UTC timestamp".into()))?;
    Ok(if micros % 1_000_000 == 0 {
        time.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        time.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    })
}
pub fn ensure(repo: &Path, platform: &mut impl Platform) -> Result<(), Error> {
    let output = platform.run(vec![
        "git".into(),
        "-C".into(),
        repo.into(),
        "show".into(),
        "-s".into(),
        "--format=%ct".into(),
        "HEAD".into(),
    ])?;
    let stamp = std::str::from_utf8(&output)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::Configuration("Invalid HEAD commit timestamp".into()))?;
    let commit = stamp as f64;
    let commit_utc = utc(commit)?;
    let now = platform.now()?;
    platform.message(&format!(
        "[build-time] device={} HEAD={commit_utc}",
        utc(now)?
    ));
    if now >= commit {
        platform.message("[build-time] Clock already meets commit floor; unchanged.");
        return Ok(());
    }
    if platform.now()? >= commit {
        platform.message("[build-time] Clock caught up; unchanged.");
        return Ok(());
    }
    let target = stamp
        .checked_add(1)
        .ok_or_else(|| Error::Configuration("commit timestamp overflow".into()))?;
    platform.message(&format!(
        "[build-time] Advancing clock to {} (offline commit floor).",
        utc(target as f64)?
    ));
    platform.run(vec![
        "sudo".into(),
        "-n".into(),
        "date".into(),
        "-u".into(),
        "-s".into(),
        format!("@{target}").into(),
    ])?;
    let after = platform.now()?;
    platform.message(&format!("[build-time] After correction: {}", utc(after)?));
    if after < commit {
        return Err(Error::Configuration(
            "System clock is still older than HEAD after correction".into(),
        ));
    }
    Ok(())
}
struct Native<'a>(&'a Paths);
impl Platform for Native<'_> {
    fn run(&mut self, argv: Vec<OsString>) -> Result<Vec<u8>, Error> {
        let output = openpilot_process_supervision::CapturedCommand {
            launcher: self.0.binary("openpilot-process-child"),
            cwd: self.0.repository.clone(),
            argv,
        }
        .capture(Duration::from_secs(10), None)
        .map_err(|e| Error::Configuration(format!("[build-time] {e}")))?;
        if !output.status.success() {
            return Err(Error::Configuration(format!(
                "[build-time] command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }
    fn now(&mut self) -> Result<f64, Error> {
        Ok(match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(value) => value.as_secs_f64(),
            Err(error) => -error.duration().as_secs_f64(),
        })
    }
    fn message(&mut self, text: &str) {
        println!("{text}");
    }
}
pub(super) fn prepare(paths: &Paths) -> Result<(), Error> {
    if paths.system.join("AGNOS").is_file() {
        ensure(&paths.repository, &mut Native(paths))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Mock {
        stamp: String,
        times: VecDeque<f64>,
        calls: Vec<Vec<OsString>>,
        messages: Vec<String>,
        fail_at: Option<usize>,
    }
    impl Platform for Mock {
        fn run(&mut self, argv: Vec<OsString>) -> Result<Vec<u8>, Error> {
            self.calls.push(argv);
            if self.fail_at == Some(self.calls.len()) {
                return Err(Error::Configuration("owned command failed".into()));
            }
            Ok(self.stamp.as_bytes().to_vec())
        }
        fn now(&mut self) -> Result<f64, Error> {
            Ok(self.times.pop_front().expect("unexpected clock read"))
        }
        fn message(&mut self, text: &str) {
            self.messages.push(text.into());
        }
    }
    #[test]
    fn original_utc_759_boundary_values() {
        #[derive(serde::Deserialize)]
        struct Case {
            bits: u64,
            utc: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../fixtures/build-time-utc-final.json"
        ))
        .unwrap();
        for (index, case) in fixture.cases.into_iter().enumerate() {
            assert_eq!(
                utc(f64::from_bits(case.bits)).ok(),
                case.utc,
                "UTCcase{index}"
            );
        }
    }
    fn mock(stamp: &str, times: &[f64]) -> Mock {
        Mock {
            stamp: stamp.into(),
            times: times.iter().copied().collect(),
            calls: Vec::new(),
            messages: Vec::new(),
            fail_at: None,
        }
    }
    #[test]
    fn original_floor_catchup_and_failure_scenarios() {
        const COMMIT: f64 = 1791158400.0;
        for now in [COMMIT, COMMIT + 1.0, COMMIT + 86400.0] {
            let mut p = mock("1791158400\n", &[now]);
            ensure(Path::new("checkout"), &mut p).unwrap();
            assert_eq!(p.calls.len(), 1);
            assert_eq!(
                p.calls[0],
                vec![
                    "git",
                    "-C",
                    "checkout",
                    "show",
                    "-s",
                    "--format=%ct",
                    "HEAD"
                ]
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
            );
        }
        let mut p = mock(
            "1791158400",
            &[COMMIT - 86400.0, COMMIT - 86400.0, COMMIT + 1.0],
        );
        ensure(Path::new("checkout"), &mut p).unwrap();
        assert_eq!(
            p.calls[1],
            vec!["sudo", "-n", "date", "-u", "-s", "@1791158401"]
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        );
        assert!(p.messages.last().unwrap().contains("After correction:"));
        let mut p = mock("1791158400", &[COMMIT - 100.0, COMMIT + 100.0]);
        ensure(Path::new("checkout"), &mut p).unwrap();
        assert_eq!(p.calls.len(), 1);
        let mut p = mock("1791158400", &[COMMIT - 100.0; 3]);
        assert!(ensure(Path::new("checkout"), &mut p).is_err());
        assert_eq!(p.calls.len(), 2);
        for stamp in ["", "invalid", "-1", "0", "99999999999999999999999"] {
            let mut p = mock(stamp, &[]);
            assert!(ensure(Path::new("checkout"), &mut p).is_err());
            assert_eq!(p.calls.len(), 1);
        }
        for failure in [1, 2] {
            let mut p = mock("1791158400", &[COMMIT - 100.0; 2]);
            p.fail_at = Some(failure);
            assert!(ensure(Path::new("checkout"), &mut p).is_err());
            assert_eq!(p.calls.len(), failure);
        }
    }
}
