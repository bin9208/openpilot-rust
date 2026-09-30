use crate::{
    at,
    config::Config,
    monotonic, observe, ppp,
    state::{registration, Snapshot, State},
    Error,
};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub struct Modem {
    pub config: Config,
    pub snapshot: Snapshot,
    pub session: ppp::Session,
    sim_change: bool,
    apn: String,
    roaming_allowed: bool,
    last_iccid_check: f64,
}
impl Modem {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            snapshot: Snapshot::default(),
            session: ppp::Session::default(),
            sim_change: false,
            apn: String::new(),
            roaming_allowed: true,
            last_iccid_check: 0.0,
        }
    }
    pub fn publish(&mut self) -> Result<(), Error> {
        self.snapshot.seconds_since_boot = monotonic();
        let parent = self
            .config
            .state
            .parent()
            .ok_or(Error::Contract("state path needs a parent"))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, &self.snapshot)?;
        file.flush()?;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o644))?;
        file.persist(&self.config.state)
            .map_err(|error| error.error)?;
        Ok(())
    }
    fn roaming(&self) -> Result<bool, Error> {
        Ok(self.snapshot.iccid.starts_with("8985235") || self.config.param("GsmRoaming")? == "1")
    }
    fn initializing(&mut self) -> Result<State, Error> {
        if !self.config.at_port.exists() {
            return Ok(State::Initializing);
        }
        self.session.kill(&self.config)?;
        ppp::cleanup(&self.config)?;
        for command in at::INIT {
            at::command(&self.config, command);
        }
        let response = at::command(&self.config, "AT+CGMI");
        if response.first().is_none_or(|line| line.starts_with("AT")) {
            return Ok(State::Initializing);
        }
        let mut identity = self.snapshot.clone();
        observe::identity(&self.config, &mut identity);
        if identity.imei.is_empty() {
            return Ok(State::Initializing);
        }
        if identity.modem_version.starts_with("EC20") || identity.modem_version.starts_with("EG25")
        {
            for command in [
                "AT+CGDCONT=0,\"IP\",\"\"",
                "AT+QSIMDET=1,0",
                "AT+QSIMSTAT=1",
                "AT+QNVW=5280,0,\"0102000000000000\"",
                "AT+QNVFW=\"/nv/item_files/ims/IMS_enable\",00",
                "AT+QNVFW=\"/nv/item_files/modem/mmode/ue_usage_setting\",01",
            ] {
                at::command(&self.config, command);
            }
        }
        self.snapshot = identity;
        self.apn = self.config.param("GsmApn")?;
        self.roaming_allowed = self.roaming()?;
        at::command(
            &self.config,
            &format!("AT+CGDCONT=1,\"IP\",\"{}\"", self.apn),
        );
        self.sim_change = false;
        self.publish()?;
        Ok(State::Searching)
    }
    fn idle(&self) -> State {
        if self.sim_change || !self.config.at_port.exists() {
            State::Disconnecting
        } else {
            State::Searching
        }
    }
    fn searching(&mut self) -> Result<State, Error> {
        if self.snapshot.sim_state == "ABSENT" {
            return Ok(self.idle());
        }
        self.roaming_allowed = self.roaming()?;
        let Some(value) = at::value(&self.config, "AT+CREG?", "+CREG:").filter(|v| !v.is_empty())
        else {
            return Ok(self.idle());
        };
        let reg = registration(&value);
        let greg =
            registration(&at::value(&self.config, "AT+CGREG?", "+CGREG:").unwrap_or_default());
        if reg == "roaming" && !self.roaming_allowed {
            self.snapshot.registration = reg.into();
            self.publish()?;
            return Ok(State::Searching);
        }
        if matches!(reg, "home" | "roaming") && matches!(greg, "home" | "roaming") {
            self.snapshot.registration = reg.into();
            self.publish()?;
            return Ok(State::Connecting);
        }
        if reg != self.snapshot.registration {
            self.snapshot.registration = reg.into();
            self.publish()?;
        }
        Ok(self.idle())
    }
    fn connected(&mut self) -> Result<State, Error> {
        if self.session.has_exited()? {
            if self.sim_change || !self.config.at_port.exists() {
                return Ok(State::Disconnecting);
            }
            self.session.fails = self.session.fails.saturating_add(1);
            if self.session.fails >= 3 {
                return Ok(State::Disconnecting);
            }
            at::reset_data_port(&self.config);
            if !self.config.at_port.exists() {
                return Ok(State::Disconnecting);
            }
            self.session.start(&self.config)?;
            return Ok(State::Connected);
        }
        if self.sim_change
            || !self.config.at_port.exists()
            || self.config.param("GsmApn")? != self.apn
            || self.roaming()? != self.roaming_allowed
        {
            return Ok(State::Disconnecting);
        }
        if observe::poll(&self.config, &mut self.snapshot, &mut self.session) {
            self.publish()?;
        }
        Ok(State::Connected)
    }
    pub fn check_iccid(&mut self, state: State) {
        if matches!(state, State::Initializing | State::Disconnecting) {
            return;
        }
        let now = monotonic();
        if now - self.last_iccid_check < self.config.iccid_interval {
            return;
        }
        self.last_iccid_check = now;
        let iccid = at::value(&self.config, "AT+QCCID", "+QCCID:")
            .unwrap_or_default()
            .trim_end_matches('F')
            .to_owned();
        if !iccid.is_empty() && iccid != self.snapshot.iccid {
            self.sim_change = true;
        }
    }
    pub fn step(&mut self, state: State) -> Result<State, Error> {
        match state {
            State::Initializing => self.initializing(),
            State::Searching => self.searching(),
            State::Connecting => {
                self.session.fails = 0;
                self.sim_change = false;
                self.session.start(&self.config)?;
                Ok(State::Connected)
            }
            State::Connected => self.connected(),
            State::Disconnecting => {
                self.snapshot = Snapshot::default();
                self.publish()?;
                self.session.kill(&self.config)?;
                ppp::cleanup(&self.config)?;
                at::reset_data_port(&self.config);
                self.sim_change = false;
                Ok(State::Initializing)
            }
        }
    }
    pub fn run(&mut self, stop: &AtomicBool) -> Result<(), Error> {
        self.snapshot.state = State::Initializing;
        self.publish()?;
        if self.config.modem_manager.is_file() {
            ppp::run(
                &self.config,
                &["systemctl", "mask", "--runtime", "ModemManager"],
            )?;
            ppp::run(&self.config, &["systemctl", "stop", "ModemManager"])?;
        }
        self.session.kill(&self.config)?;
        let mut state = State::Initializing;
        while !stop.load(Ordering::Relaxed) {
            self.check_iccid(state);
            let result = self.step(state).and_then(|next| {
                if next != state {
                    self.snapshot.state = next;
                    self.publish()?;
                    eprintln!("modem: state transition: {state:?} -> {next:?}");
                }
                Ok(next)
            });
            state = match result {
                Ok(next) => next,
                Err(error) => {
                    eprintln!("modem: error in {state:?}: {error}");
                    State::Disconnecting
                }
            };
            std::thread::sleep(Duration::from_millis(self.config.state_wait_ms));
        }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        self.session.kill(&self.config)?;
        ppp::cleanup(&self.config)?;
        match fs::remove_file(&self.config.state) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if self.config.modem_manager.is_file() {
            ppp::run(
                &self.config,
                &["systemctl", "unmask", "--runtime", "ModemManager"],
            )?;
            ppp::run(&self.config, &["systemctl", "start", "ModemManager"])?;
        }
        Ok(())
    }
}
