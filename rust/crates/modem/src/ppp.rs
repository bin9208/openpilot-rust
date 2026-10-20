use crate::{
    config::Config,
    helper::{self, Child, NEVER_CANCELLED},
    Error,
};
use std::ffi::OsString;
use std::{net::Ipv4Addr, process::Stdio, sync::atomic::AtomicBool};

#[derive(Default)]
pub struct Session {
    process: Option<Child>,
    pub fails: u32,
    peer: String,
}
impl Session {
    pub fn start(&mut self, config: &Config) -> Result<(), Error> {
        self.start_controlled(config, &NEVER_CANCELLED)
    }
    pub(crate) fn start_controlled(
        &mut self,
        config: &Config,
        cancelled: &AtomicBool,
    ) -> Result<(), Error> {
        helper::check(cancelled)?;
        let mut argv: Vec<OsString> = vec![
            config.sudo.clone().into(),
            "pppd".into(),
            config.ppp_port.clone().into(),
        ];
        argv.extend([
            "460800", "noauth", "nodetach", "noipdefault", "usepeerdns", "nodefaultroute", "connect",
            "/usr/sbin/chat -v ABORT 'NO CARRIER' ABORT 'NO DIALTONE' ABORT 'BUSY' ABORT 'NO ANSWER' ABORT 'ERROR' TIMEOUT 5 '' AT OK ATD*99***1# CONNECT ''",
            "lcp-echo-interval", "30", "lcp-echo-failure", "4", "mtu", "1500", "mru", "1500", "novj", "novjccomp",
            "ipcp-accept-local", "ipcp-accept-remote", "nomagic", "user", "\"\"", "password", "\"\"",
        ].into_iter().map(OsString::from));
        self.process = None;
        self.process = Some(Child::spawn(config, argv, Stdio::null(), Stdio::null())?);
        self.peer.clear();
        Ok(())
    }
    pub fn kill(&mut self, config: &Config) -> Result<(), Error> {
        self.kill_controlled(config, &NEVER_CANCELLED)
    }
    pub(crate) fn kill_controlled(
        &mut self,
        config: &Config,
        cancelled: &AtomicBool,
    ) -> Result<(), Error> {
        let result = run_controlled(config, &["killall", "-9", "pppd"], cancelled);
        // Always terminate/reap our PPP session, including when sudo failed or cancellation won.
        // Keep the completed handle: has_exited distinguishes killed PPP from never started.
        let cleanup = self
            .process
            .as_mut()
            .map(Child::terminate)
            .transpose()
            .map(|_| ());
        self.peer.clear();
        result.map(|_| ()).and(cleanup.map_err(Error::from))
    }
    pub fn has_exited(&mut self) -> Result<bool, Error> {
        match self.process.as_mut() {
            Some(child) => Ok(child.exited()?),
            None => Ok(false),
        }
    }
    pub fn routes(&mut self, config: &Config, ip: &str, peer: &str) -> Result<bool, Error> {
        self.routes_controlled(config, ip, peer, &NEVER_CANCELLED)
    }
    pub(crate) fn routes_controlled(
        &mut self,
        config: &Config,
        ip: &str,
        peer: &str,
        cancelled: &AtomicBool,
    ) -> Result<bool, Error> {
        if peer.is_empty() || peer == self.peer {
            return Ok(false);
        }
        if ip.parse::<Ipv4Addr>().is_err() || peer.parse::<Ipv4Addr>().is_err() {
            self.kill_controlled(config, cancelled)?;
            return Ok(false);
        }
        cleanup_controlled(config, cancelled)?;
        for args in [
            vec![
                "ip", "route", "add", "default", "via", peer, "dev", "ppp0", "metric", "1000",
            ],
            vec![
                "ip", "route", "add", "default", "via", peer, "dev", "ppp0", "table", "1000",
            ],
            vec!["ip", "rule", "add", "from", ip, "table", "1000"],
        ] {
            if !run_controlled(config, &args, cancelled)? {
                cleanup_controlled(config, cancelled)?;
                self.kill_controlled(config, cancelled)?;
                return Ok(false);
            }
        }
        self.peer = peer.to_owned();
        Ok(true)
    }
    pub fn dns(&mut self, config: &Config, servers: &[String]) -> Result<bool, Error> {
        self.dns_controlled(config, servers, &NEVER_CANCELLED)
    }
    pub(crate) fn dns_controlled(
        &mut self,
        config: &Config,
        servers: &[String],
        cancelled: &AtomicBool,
    ) -> Result<bool, Error> {
        if servers.is_empty() {
            return Ok(false);
        }
        let mut args = vec!["resolvectl", "dns", "ppp0"];
        args.extend(servers.iter().map(String::as_str));
        if !run_controlled(config, &args, cancelled)?
            || !run_controlled(
                config,
                &["resolvectl", "default-route", "ppp0", "yes"],
                cancelled,
            )?
        {
            self.kill_controlled(config, cancelled)?;
            return Ok(false);
        }
        Ok(true)
    }
}
pub fn run(config: &Config, args: &[&str]) -> Result<bool, Error> {
    run_controlled(config, args, &NEVER_CANCELLED)
}
pub fn run_controlled(
    config: &Config,
    args: &[&str],
    cancelled: &AtomicBool,
) -> Result<bool, Error> {
    helper::check(cancelled)?;
    let mut argv = vec![config.sudo.clone().into_os_string()];
    argv.extend(args.iter().map(OsString::from));
    Child::spawn(config, argv, Stdio::null(), Stdio::null())?.wait(cancelled)
}
pub fn cleanup(config: &Config) -> Result<(), Error> {
    cleanup_controlled(config, &NEVER_CANCELLED)
}
pub(crate) fn cleanup_controlled(config: &Config, cancelled: &AtomicBool) -> Result<(), Error> {
    run_controlled(
        config,
        &["ip", "route", "del", "default", "dev", "ppp0"],
        cancelled,
    )?;
    run_controlled(
        config,
        &["ip", "route", "flush", "table", "1000"],
        cancelled,
    )?;
    while run_controlled(config, &["ip", "rule", "del", "table", "1000"], cancelled)? {}
    run_controlled(config, &["resolvectl", "revert", "ppp0"], cancelled)?;
    Ok(())
}
pub fn interface(config: &Config) -> Result<(String, String), Error> {
    interface_controlled(config, &NEVER_CANCELLED)
}
pub fn interface_controlled(
    config: &Config,
    cancelled: &AtomicBool,
) -> Result<(String, String), Error> {
    let text = helper::interface(config, cancelled)?;
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
