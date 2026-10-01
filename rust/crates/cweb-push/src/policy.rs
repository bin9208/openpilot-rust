use crate::{helpers, Config, Error, Payload, Platform, State, Status, StatusKind};

pub struct Reporter {
    pub config: Config,
    pub state: State,
}
impl Reporter {
    pub fn new(mut config: Config, io: &mut impl Platform) -> Self {
        config.heartbeat_interval_s = helpers::maximum(config.heartbeat_interval_s, 5.);
        let next_heartbeat_at = io.monotonic() + io.uniform(0., config.heartbeat_interval_s);
        Self {
            config,
            state: State {
                last_success_ip: String::new(),
                current_candidate_ip: String::new(),
                current_candidate_since: 0.,
                was_down: true,
                first_report: true,
                next_retry_at: 0.,
                backoff_s: 5.,
                next_heartbeat_at,
            },
        }
    }
    pub fn status(&self, state: StatusKind, io: &impl Platform) -> Status {
        Status {
            state,
            ts: io.wall_seconds() as i64,
            last_success_ip: self.state.last_success_ip.clone(),
            ip: None,
            payload: None,
            http_status: None,
            response: None,
            error: None,
            retry_in_s: None,
        }
    }
    fn payload(&self, ip: &str, io: &mut impl Platform) -> Payload {
        Payload {
            device_id: io.device_id(),
            ip: ip.into(),
            port: self.config.port,
        }
    }
    pub fn poll_once(&mut self, io: &mut impl Platform) -> Result<bool, Error> {
        let now = io.monotonic();
        let ip = io.local_ip(&self.config.iface);
        if ip.is_empty() {
            self.state.was_down = true;
            self.state.current_candidate_ip.clear();
            self.state.current_candidate_since = 0.;
            io.emit(self.status(StatusKind::NoIp, io))?;
            return Ok(false);
        }
        if ip != self.state.current_candidate_ip {
            self.state.current_candidate_ip.clone_from(&ip);
            self.state.current_candidate_since = now;
            io.emit(self.status(StatusKind::IpCandidate, io).ip(&ip))?;
            return Ok(false);
        }
        if now - self.state.current_candidate_since < self.config.debounce_s {
            return Ok(false);
        }
        if !self.state.first_report && ip == self.state.last_success_ip {
            let heartbeat_due = now >= self.state.next_heartbeat_at;
            if !heartbeat_due {
                io.emit(self.status(StatusKind::Idle, io).ip(&ip))?;
                return Ok(false);
            }
            let payload = self.payload(&ip, io);
            self.state.next_heartbeat_at = now
                + self.config.heartbeat_interval_s
                + io.uniform(
                    0.,
                    helpers::minimum(3., self.config.heartbeat_interval_s * 0.15),
                );
            if self.config.dry_run {
                io.emit(
                    self.status(StatusKind::HeartbeatDryRun, io)
                        .ip(&ip)
                        .payload(payload),
                )?;
                return Ok(true);
            }
            let result = io.post(&self.config.heartbeat_url, &payload, self.config.timeout_s)?;
            let ok = result.ok;
            let kind = if ok {
                StatusKind::Heartbeat
            } else {
                StatusKind::HeartbeatFailed
            };
            io.emit(self.status(kind, io).ip(&ip).response(result))?;
            return Ok(ok);
        }
        if now < self.state.next_retry_at {
            return Ok(false);
        }
        let payload = self.payload(&ip, io);
        if self.config.dry_run {
            self.state.last_success_ip.clone_from(&ip);
            self.state.was_down = false;
            self.state.first_report = false;
            io.emit(self.status(StatusKind::DryRun, io).ip(&ip).payload(payload))?;
            return Ok(true);
        }
        let result = io.post(&self.config.report_url, &payload, self.config.timeout_s)?;
        if result.ok {
            self.state.last_success_ip.clone_from(&ip);
            self.state.was_down = false;
            self.state.first_report = false;
            self.state.backoff_s = 5.;
            self.state.next_retry_at = 0.;
            self.state.next_heartbeat_at = now + self.config.heartbeat_interval_s;
            io.emit(
                self.status(StatusKind::Reported, io)
                    .ip(&ip)
                    .response(result),
            )?;
            Ok(true)
        } else {
            self.state.next_retry_at = now + self.state.backoff_s;
            self.state.backoff_s = helpers::minimum(self.state.backoff_s * 2., 180.);
            io.emit(
                self.status(StatusKind::ReportFailed, io)
                    .ip(&ip)
                    .failure(result, self.state.backoff_s),
            )?;
            Ok(false)
        }
    }
}
