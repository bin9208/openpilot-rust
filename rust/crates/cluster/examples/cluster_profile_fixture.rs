use openpilot_cluster::runtime::profile::Reporter;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    enabled: bool,
    interval: f64,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Operation {
    Report {
        report: f64,
    },
    Sample {
        name: String,
        sample: f64,
        frame: bool,
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args().nth(1).ok_or("profile fixture INPUT")?;
    let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut results = Vec::new();
    for case in cases {
        let mut reporter = Reporter::new(case.enabled, case.interval, 0.0);
        let mut reports = Vec::new();
        for operation in case.operations {
            match operation {
                Operation::Report { report } => reports.push(
                    reporter
                        .report(report, "owned native summary")
                        .unwrap_or_default(),
                ),
                Operation::Sample {
                    name,
                    sample,
                    frame,
                } => {
                    reporter.add(&name, sample);
                    if frame {
                        reporter.frame_done();
                    }
                }
            }
        }
        results.push(reports);
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
