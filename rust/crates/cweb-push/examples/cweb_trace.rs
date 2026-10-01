use openpilot_cweb_push::{
    helpers, Config, Error, Payload, Platform, PostResult, Reporter, Status,
};
use serde::Deserialize;
use std::io::{BufRead, Write};

#[derive(Deserialize)]
struct Frame {
    now: f64,
    wall: f64,
    ip: String,
    id: String,
    fraction: f64,
    result: PostResult,
}
#[derive(Deserialize)]
struct Request {
    config: Config,
    start: f64,
    fraction: f64,
    frames: Vec<Frame>,
    #[serde(default)]
    helpers: Vec<String>,
}
struct Input {
    frame: Frame,
    statuses: Vec<Status>,
    calls: Vec<serde_json::Value>,
}
impl Platform for Input {
    fn monotonic(&self) -> f64 {
        self.frame.now
    }
    fn wall_seconds(&self) -> f64 {
        self.frame.wall
    }
    fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.frame.fraction
    }
    fn local_ip(&mut self, _: &str) -> String {
        self.frame.ip.clone()
    }
    fn device_id(&mut self) -> String {
        self.frame.id.clone()
    }
    fn post(&mut self, url: &str, payload: &Payload, timeout: f64) -> Result<PostResult, Error> {
        self.calls
            .push(serde_json::json!({"url":url,"payload":payload,"timeout":timeout}));
        Ok(self.frame.result.clone())
    }
    fn emit(&mut self, status: Status) -> Result<(), Error> {
        self.statuses.push(status);
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let mut input = Input {
            frame: Frame {
                now: request.start,
                wall: 0.,
                ip: String::new(),
                id: String::new(),
                fraction: request.fraction,
                result: PostResult {
                    ok: false,
                    status: 0,
                    body: String::new(),
                },
            },
            statuses: Vec::new(),
            calls: Vec::new(),
        };
        let mut reporter = Reporter::new(request.config, &mut input);
        let initial = serde_json::to_value(&reporter.state)?;
        let mut rows = Vec::new();
        for frame in request.frames {
            input.frame = frame;
            let result = reporter.poll_once(&mut input)?;
            rows.push(serde_json::json!({"result":result,"state":reporter.state,"statuses":std::mem::take(&mut input.statuses),"calls":std::mem::take(&mut input.calls)}));
        }
        let helpers: Vec<_> = request.helpers.iter().map(|text| serde_json::json!({"strip":helpers::strip(text),"ip":helpers::usable_ip(text),
            "id":helpers::meaningful_id(text),"heartbeat":helpers::heartbeat_url(text),"notify":helpers::notify_url(text)})).collect();
        writeln!(
            output,
            "{}",
            serde_json::json!({"initial":initial,"rows":rows,"helpers":helpers})
        )?;
    }
    Ok(())
}
