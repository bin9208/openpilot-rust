use crate::{config::Config, Error};
use std::{
    io::{Read, Seek},
    net::Ipv4Addr,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Session {
    process: Option<Child>,
    pub fails: u32,
    peer: String,
}
impl Session {
    pub fn start(&mut self, config: &Config) -> Result<(), Error> {
        self.process = Some(Command::new(&config.sudo).arg("pppd").arg(&config.ppp_port).args([
            "460800", "noauth", "nodetach", "noipdefault", "usepeerdns", "nodefaultroute", "connect",
            "/usr/sbin/chat -v ABORT 'NO CARRIER' ABORT 'NO DIALTONE' ABORT 'BUSY' ABORT 'NO ANSWER' ABORT 'ERROR' TIMEOUT 5 '' AT OK ATD*99***1# CONNECT ''",
            "lcp-echo-interval", "30", "lcp-echo-failure", "4", "mtu", "1500", "mru", "1500", "novj", "novjccomp",
            "ipcp-accept-local", "ipcp-accept-remote", "nomagic", "user", "\"\"", "password", "\"\"",
        ]).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?);
        self.peer.clear();
        Ok(())
    }
    pub fn kill(&mut self, config: &Config) -> Result<(), Error> {
        run(config, &["killall", "-9", "pppd"])?;
        // Reap the owned pppd/sudo child; never leave a zombie after reconnect.
        if let Some(child) = self.process.as_mut() {
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
        }
        self.peer.clear();
        Ok(())
    }
    pub fn has_exited(&mut self) -> Result<bool, Error> {
        match self.process.as_mut() {
            Some(child) => Ok(child.try_wait()?.is_some()),
            None => Ok(false),
        }
    }
    pub fn routes(&mut self, config: &Config, ip: &str, peer: &str) -> Result<bool, Error> {
        if peer.is_empty() || peer == self.peer {
            return Ok(false);
        }
        if ip.parse::<Ipv4Addr>().is_err() || peer.parse::<Ipv4Addr>().is_err() {
            self.kill(config)?;
            return Ok(false);
        }
        cleanup(config)?;
        for args in [
            vec![
                "ip", "route", "add", "default", "via", peer, "dev", "ppp0", "metric", "1000",
            ],
            vec![
                "ip", "route", "add", "default", "via", peer, "dev", "ppp0", "table", "1000",
            ],
            vec!["ip", "rule", "add", "from", ip, "table", "1000"],
        ] {
            if !run(config, &args)? {
                cleanup(config)?;
                self.kill(config)?;
                return Ok(false);
            }
        }
        self.peer = peer.to_owned();
        Ok(true)
    }
    pub fn dns(&mut self, config: &Config, servers: &[String]) -> Result<bool, Error> {
        if servers.is_empty() {
            return Ok(false);
        }
        let mut args = vec!["resolvectl", "dns", "ppp0"];
        args.extend(servers.iter().map(String::as_str));
        if !run(config, &args)? || !run(config, &["resolvectl", "default-route", "ppp0", "yes"])? {
            self.kill(config)?;
            return Ok(false);
        }
        Ok(true)
    }
}
pub fn run(config: &Config, args: &[&str]) -> Result<bool, Error> {
    Ok(Command::new(&config.sudo)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success())
}
pub fn cleanup(config: &Config) -> Result<(), Error> {
    run(config, &["ip", "route", "del", "default", "dev", "ppp0"])?;
    run(config, &["ip", "route", "flush", "table", "1000"])?;
    while run(config, &["ip", "rule", "del", "table", "1000"])? {}
    run(config, &["resolvectl", "revert", "ppp0"])?;
    Ok(())
}
pub fn interface(config: &Config) -> Result<(String, String), Error> {
    let mut output = tempfile::tempfile()?;
    let mut child = Command::new(&config.ip)
        .args(["-4", "addr", "show", "ppp0"])
        .stdout(output.try_clone()?)
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err(
                std::io::Error::new(std::io::ErrorKind::TimedOut, "ip addr timeout").into(),
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    output.rewind()?;
    let mut text = String::new();
    output.read_to_string(&mut text)?;
    for line in text.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        if let Some(index) = parts.iter().position(|part| *part == "inet") {
            let address = |index: usize| {
                parts
                    .get(index + 1)
                    .and_then(|v| v.split('/').next())
                    .unwrap_or_default()
                    .to_owned()
            };
            return Ok((
                address(index),
                parts
                    .iter()
                    .position(|part| *part == "peer")
                    .map(address)
                    .unwrap_or_default(),
            ));
        }
    }
    Ok((String::new(), String::new()))
}
