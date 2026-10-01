use openpilot_cweb_push::{
    Config, Error, Payload, Platform, PostResult, Reporter, Status, StatusKind,
};

#[derive(Default)]
struct Fake {
    now: f64,
    ip: String,
    success: bool,
    posts: Vec<String>,
    statuses: Vec<Status>,
}
impl Platform for Fake {
    fn monotonic(&self) -> f64 {
        self.now
    }
    fn wall_seconds(&self) -> f64 {
        1000.9
    }
    fn uniform(&mut self, low: f64, high: f64) -> f64 {
        (low + high) / 2.
    }
    fn local_ip(&mut self, _: &str) -> String {
        self.ip.clone()
    }
    fn device_id(&mut self) -> String {
        "fixture".into()
    }
    fn post(&mut self, url: &str, _: &Payload, _: f64) -> Result<PostResult, Error> {
        self.posts.push(url.into());
        Ok(PostResult {
            ok: self.success,
            status: if self.success { 200 } else { 503 },
            body: "response".into(),
        })
    }
    fn emit(&mut self, status: Status) -> Result<(), Error> {
        self.statuses.push(status);
        Ok(())
    }
}

#[test]
fn retry_waits_for_previous_backoff_but_reports_next_backoff() {
    let mut io = Fake {
        ip: "192.168.1.2".into(),
        ..Fake::default()
    };
    let mut reporter = Reporter::new(Config::default(), &mut io);
    assert!(!reporter.poll_once(&mut io).unwrap());
    io.now = 1.;
    assert!(!reporter.poll_once(&mut io).unwrap());
    assert_eq!(reporter.state.next_retry_at, 6.);
    assert_eq!(reporter.state.backoff_s, 10.);
    assert_eq!(io.statuses.last().unwrap().retry_in_s, Some(10.));
    io.now = 5.9;
    assert!(!reporter.poll_once(&mut io).unwrap());
    assert_eq!(io.posts.len(), 1);
    io.now = 6.;
    io.success = true;
    assert!(reporter.poll_once(&mut io).unwrap());
    assert_eq!(reporter.state.backoff_s, 5.);
}

#[test]
fn same_ip_after_outage_uses_heartbeat_not_new_report() {
    let mut io = Fake {
        ip: "192.168.1.2".into(),
        success: true,
        ..Fake::default()
    };
    let mut reporter = Reporter::new(Config::default(), &mut io);
    reporter.poll_once(&mut io).unwrap();
    io.now = 1.;
    reporter.poll_once(&mut io).unwrap();
    io.now = 2.;
    io.ip.clear();
    reporter.poll_once(&mut io).unwrap();
    io.now = 20.;
    io.ip = "192.168.1.2".into();
    reporter.poll_once(&mut io).unwrap();
    io.now = 21.;
    assert!(reporter.poll_once(&mut io).unwrap());
    assert_eq!(io.statuses.last().unwrap().state, StatusKind::Heartbeat);
    assert_eq!(io.posts.len(), 2);
}
