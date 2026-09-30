use crate::{
    at,
    config::Config,
    ppp,
    state::{network_type, Snapshot},
};
use std::{fs, net::Ipv4Addr};

pub fn identity(config: &Config, state: &mut Snapshot) {
    let first = |cmd| {
        at::command(config, cmd)
            .into_iter()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let digits = |v: &str| !v.is_empty() && v.chars().all(|ch| ch.is_ascii_digit());
    let imei = first("AT+CGSN");
    state.imei = if digits(&imei) && (14..=17).contains(&imei.len()) {
        imei
    } else {
        String::new()
    };
    let iccid = at::value(config, "AT+QCCID", "+QCCID:")
        .unwrap_or_default()
        .trim_end_matches('F')
        .to_owned();
    state.iccid = if digits(&iccid) { iccid } else { String::new() };
    let imsi = first("AT+CIMI");
    state.mcc_mnc = if digits(&imsi) && imsi.len() >= 6 {
        imsi[..6].to_owned()
    } else {
        String::new()
    };
    state.sim_state = if state.iccid.is_empty() && state.mcc_mnc.is_empty() {
        "ABSENT"
    } else {
        "READY"
    }
    .into();
    state.modem_version = first("AT+GMR");
}

pub fn radio(config: &Config, state: &mut Snapshot) -> bool {
    let mut changed = false;
    if let Some(v) = at::value(config, "AT+CSQ", "+CSQ:") {
        if let Some(rssi) = v
            .split(',')
            .next()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v != 99)
        {
            state.signal_strength = rssi;
            state.signal_quality = rssi
                .saturating_mul(100)
                .checked_div(31)
                .unwrap_or_default()
                .min(100);
            changed = true;
        }
    }
    if let Some(v) = at::value(config, "AT+COPS?", "+COPS:") {
        let parts: Vec<_> = v.split(',').collect();
        if let Some(operator) = parts.get(2) {
            state.operator = operator.trim_matches('"').to_string();
            changed = true;
        }
        if let Some(kind) = parts.get(3).and_then(|v| v.trim().parse().ok()) {
            state.network_type = network_type(kind).into();
            changed = true;
        }
    }
    if let Some(v) = at::value(config, "AT+QNWINFO", "+QNWINFO:") {
        let stripped = v.replace('"', "");
        let parts: Vec<_> = stripped.split(',').collect();
        if let Some(channel) = parts.get(3).and_then(|v| v.trim().parse().ok()) {
            state.band = parts[2].into();
            state.channel = channel;
            changed = true;
        }
    }
    if let Some(v) =
        at::value(config, "AT+QENG=\"servingcell\"", "+QENG:").filter(|v| !v.is_empty())
    {
        state.extra = v.replace('"', "");
        changed = true;
    }
    if let Some(v) = at::value(config, "AT+QTEMP", "+QTEMP:").filter(|v| !v.is_empty()) {
        if let Ok(temps) = v
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().parse::<i64>())
            .collect::<Result<Vec<_>, _>>()
        {
            state.temperatures = temps.into_iter().filter(|t| *t != 255).collect();
            changed = true;
        }
    }
    changed
}
pub fn dns(config: &Config) -> Vec<String> {
    at::value(config, "AT+CGCONTRDP=1", "+CGCONTRDP:")
        .unwrap_or_default()
        .split(',')
        .skip(5)
        .take(2)
        .filter_map(|v| v.trim().trim_matches('"').parse::<Ipv4Addr>().ok())
        .map(|v| v.to_string())
        .collect()
}
pub fn poll(config: &Config, state: &mut Snapshot, session: &mut ppp::Session) -> bool {
    let mut changed = radio(config, state);
    // Source catches interface/route exceptions independently of radio and byte counters.
    match poll_interface(config, state, session) {
        Ok(updated) => changed |= updated,
        Err(error) => eprintln!("modem: interface poll failed: {error}"),
    }
    let counter = |key| {
        fs::read_to_string(config.statistics.join(key))
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
    };
    if let (Some(tx), Some(rx)) = (counter("tx_bytes"), counter("rx_bytes")) {
        state.tx_bytes = tx;
        state.rx_bytes = rx;
        changed = true;
    }
    changed
}
fn poll_interface(
    config: &Config,
    state: &mut Snapshot,
    session: &mut ppp::Session,
) -> Result<bool, crate::Error> {
    let (ip, peer) = ppp::interface(config)?;
    if !ip.is_empty() {
        if session.routes(config, &ip, &peer)? {
            session.dns(config, &dns(config))?;
        }
        state.ip_address = ip;
        state.connected = true;
        return Ok(true);
    }
    if state.connected {
        state.connected = false;
        state.ip_address.clear();
        return Ok(true);
    }
    Ok(false)
}
