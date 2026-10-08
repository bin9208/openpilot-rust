use super::{
    bus::Inputs,
    config::Config,
    parameters::{Write, Writes},
};
use crate::{owner::Owner, Error};
use openpilot_params::Params;
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};

pub fn local_ip(destination: Ipv4Addr) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((destination, 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        std::net::IpAddr::V6(_) => None,
    }
}
fn broadcast_ip(local: Option<Ipv4Addr>) -> Ipv4Addr {
    let Ok(interfaces) = nix::ifaddrs::getifaddrs() else {
        return Ipv4Addr::BROADCAST;
    };
    let mut interfaces: Vec<_> = interfaces
        .filter(|i| {
            i.address
                .as_ref()
                .and_then(|a| a.as_sockaddr_in())
                .is_some_and(|a| !a.ip().is_loopback())
        })
        .collect();
    interfaces.sort_by_key(|i| {
        i.address
            .as_ref()
            .and_then(|a| a.as_sockaddr_in())
            .map(|a| a.ip())
            != local
    });
    for interface in interfaces {
        if let Some(broadcast) = interface
            .broadcast
            .as_ref()
            .and_then(|a| a.as_sockaddr_in())
        {
            return broadcast.ip();
        }
        if let (Some(address), Some(mask)) = (
            interface.address.as_ref().and_then(|a| a.as_sockaddr_in()),
            interface.netmask.as_ref().and_then(|a| a.as_sockaddr_in()),
        ) {
            return Ipv4Addr::from(u32::from(address.ip()) | !u32::from(mask.ip()));
        }
    }
    Ipv4Addr::BROADCAST
}

pub struct Broadcast {
    socket: UdpSocket,
    next: f64,
    last_error_log: f64,
}
impl Broadcast {
    pub fn new() -> Result<Self, Error> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        Ok(Self {
            socket,
            next: 0.,
            last_error_log: 0.,
        })
    }
    pub fn send(
        &mut self,
        owner: &mut Owner,
        inputs: &Inputs,
        params: &Params,
        writes: &Writes,
        config: &Config,
        now: f64,
    ) -> Result<(), Error> {
        if now < self.next {
            return Ok(());
        }
        let remote = owner.peers.selected();
        self.next = now + if remote.is_some() { 0.2 } else { 1. };
        let local = if !config.bind.is_unspecified() {
            Some(config.bind)
        } else if std::path::Path::new("/TICI").is_file() {
            nix::unistd::gethostname().ok().and_then(|name| {
                name.to_str()
                    .and_then(|name| (name, 0).to_socket_addrs().ok())
                    .and_then(|mut addresses| {
                        addresses.find_map(|a| {
                            if let std::net::IpAddr::V4(ip) = a.ip() {
                                Some(ip)
                            } else {
                                None
                            }
                        })
                    })
            })
        } else {
            local_ip(Ipv4Addr::new(8, 8, 8, 8))
        };
        let result = (|| {
            let local = local.ok_or_else(|| std::io::Error::from_raw_os_error(101))?;
            owner.ip_address = local.to_string();
            writes.send(Write::Network(owner.ip_address.clone()))?;
            let target = if let Some(remote) = remote {
                remote.ip()
            } else if !config.bind.is_unspecified() {
                std::net::IpAddr::V4(config.bind)
            } else {
                std::net::IpAddr::V4(broadcast_ip(Some(local)))
            };
            let payload = message(owner, inputs, params, config)?;
            self.socket.send_to(
                payload.as_bytes(),
                SocketAddr::new(target, config.broadcast_port),
            )?;
            if remote.is_none() && !owner.route.navd_active {
                owner.route.points.clear();
                owner.route.active = false;
            }
            Ok::<_, Error>(())
        })();
        if let Err(Error::Io(error)) = &result {
            if matches!(error.raw_os_error(), Some(100 | 101 | 113)) {
                owner
                    .peers
                    .clear_fallback(crate::ingress::peers::Fallback::Udp);
                owner.ip_address = "0.0.0.0".into();
                writes.send(Write::Network(owner.ip_address.clone()))?;
                self.next = now + 5.;
                if now - self.last_error_log >= 30. {
                    eprintln!("carrot_man broadcast skipped: {error}");
                    self.last_error_log = now;
                }
                return Ok(());
            }
        }
        if let Err(error) = result {
            super::diagnostics::queue_exception(params, writes, "tmux_send");
            eprintln!("carrot_man broadcast error: {error}");
        }
        Ok(())
    }
}

fn message(
    owner: &Owner,
    inputs: &Inputs,
    params: &Params,
    config: &Config,
) -> Result<String, Error> {
    let onroad = params.get_bool("IsOnroad")?;
    let car = inputs.car.as_ref().filter(|_| onroad);
    let v_ego = car.map_or(0., |c| (c.v_ego_cluster * 3.6 + 0.5).trunc());
    let version = params
        .get("Version")?
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    Ok(serde_json::json!({"Carrot2":version,"IsOnroad":onroad,"CarrotRouteActive":owner.route.active,"ip":owner.ip_address,"port":config.udp_port,"navi_debug":0,"navi_http_port":config.http_port,
        "log_carrot":car.map_or("",|c|c.log_carrot.as_str()),"v_cruise_kph":car.map_or(0.,|c|c.v_cruise),"carcruiseSpeed":car.map_or(0.,|c|c.cruise_speed*3.6),"v_ego_kph":v_ego,"tbt_dist":owner.serv.nav.x_turn_distance,"sdi_dist":owner.serv.nav.speed_distance,
        "active":onroad&&inputs.selfdrive_active,"xState":if onroad{inputs.x_state}else{0},"trafficState":if onroad{inputs.traffic_state}else{0}}).to_string())
}
