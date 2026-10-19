//! USB-PD owner policy from zoompilot/jetlink/comma/port.py (MIT, 2026).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Off,
    Hold,
    Device,
    Reset,
    Start,
    Stop,
    Source,
    Sink,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "port-off",
            Self::Hold => "port-hold",
            Self::Device => "port-device",
            Self::Reset => "port-reset",
            Self::Start => "udc-start",
            Self::Stop => "udc-stop",
            Self::Source => "port-source",
            Self::Sink => "port-sink",
        }
    }
}

pub struct Input<'a> {
    pub now: f64,
    pub role: Option<&'a str>,
    pub data: Option<&'a str>,
    pub configured: bool,
    pub host_attached: bool,
    pub egpu: bool,
    pub udc_mode: &'a str,
    pub charge: bool,
    pub contract: &'a str,
    pub hottest_c: Option<f64>,
}

#[derive(Debug)]
pub struct Port {
    pub cleared: bool,
    pub deferred: bool,
    pub held: bool,
    pub settled: bool,
    pub role: Option<String>,
    pub role_since: f64,
    pub roles: (Option<String>, Option<String>),
    pub roles_since: f64,
    pub plugged: bool,
    pub asks: u32,
    pub next_ask: f64,
    pub next_start: f64,
    pub charge_asked: bool,
    pub charging: bool,
    pub charge_since: f64,
    pub said_hot: bool,
    pub temperature: Option<f64>,
}
impl Default for Port {
    fn default() -> Self {
        Self {
            cleared: false,
            deferred: false,
            held: false,
            settled: false,
            role: None,
            role_since: 0.,
            roles: (None, None),
            roles_since: 0.,
            plugged: true,
            asks: 0,
            next_ask: 0.,
            next_start: 0.,
            charge_asked: false,
            charging: false,
            charge_since: 0.,
            said_hot: false,
            temperature: None,
        }
    }
}
impl Port {
    pub fn update(&mut self, input: Input<'_>, mut run: impl FnMut(Action) -> bool) {
        if !self.cleared {
            if input.host_attached {
                self.deferred = true;
            } else {
                run(Action::Off);
                self.cleared = true;
            }
        }
        let now = input.now;
        let role = input.role;
        let attached = matches!(role, Some("source" | "sink"));
        let data = if attached { input.data } else { None };
        if (role, data) != (self.roles.0.as_deref(), self.roles.1.as_deref()) {
            if self.charging && (role, data) == (Some("sink"), Some("ufp")) {
                self.charging = false;
            }
            self.roles = (role.map(str::to_owned), data.map(str::to_owned));
            self.roles_since = now;
        }
        if role != self.role.as_deref() {
            self.role = role.map(str::to_owned);
            self.role_since = now;
        }
        let lasted = now - self.role_since;
        if attached {
            self.plugged = true;
        } else if self.plugged && lasted >= 1.5 {
            self.plugged = false;
            self.asks = 0;
            self.charge_asked = false;
            self.charging = false;
            self.said_hot = false;
            self.temperature = None;
            if input.udc_mode == "peripheral" {
                run(Action::Stop);
            }
        }
        if role == Some("sink") && data == Some("ufp") {
            if !input.configured {
                if now - self.roles_since >= 3.
                    && now >= self.next_start
                    && !matches!(input.udc_mode, "" | "peripheral")
                {
                    self.next_start = now + 10.;
                    run(Action::Start);
                }
            } else if input.charge
                && !self.charge_asked
                && now - self.roles_since >= 3.
                && input.contract == "explicit"
            {
                let mut hot = false;
                if let Some(temp) = input.hottest_c {
                    let smoothed = self
                        .temperature
                        .map_or(temp, |prior| prior + (temp - prior) * 0.2);
                    self.temperature = Some(smoothed);
                    hot = smoothed >= 90.;
                    if hot {
                        self.said_hot = true;
                    }
                }
                if !hot {
                    self.charge_asked = true;
                    if self.held {
                        self.held = false;
                        run(Action::Off);
                    }
                    if run(Action::Source) {
                        self.charging = true;
                        self.charge_since = now;
                    }
                }
            }
        }
        if self.charging && !input.configured && role == Some("source") {
            self.charging = false;
            run(Action::Sink);
        }
        if self.held {
            if role == Some("sink") {
                self.settled = false;
            } else if role != Some("source") && lasted >= 5. {
                self.held = false;
                run(Action::Off);
                self.role_since = now;
                self.plugged = true;
            }
        } else if data == Some("dfp") || (role == Some("source") && data != Some("ufp")) {
            if !self.settled && now - self.roles_since >= 3. && now >= self.next_ask {
                if self.asks == 0 && input.egpu {
                    self.settled = true;
                } else if role == Some("source") {
                    self.held = run(Action::Hold);
                    self.settled = true;
                } else {
                    self.asks += 1;
                    self.next_ask = now + 2.;
                    match self.asks {
                        1..=3 => {
                            run(Action::Device);
                        }
                        4 => {
                            run(Action::Reset);
                        }
                        _ => self.settled = true,
                    }
                }
            }
        } else if role == Some("sink") || !self.plugged {
            self.settled = false;
        }
    }

    pub fn off(&mut self, mut run: impl FnMut(Action) -> bool) {
        if self.held {
            run(Action::Off);
        }
        *self = Self::default();
    }
}
