use crate::{json::Value, Error};
use nix::{
    ifaddrs::getifaddrs,
    sys::socket::{SockaddrLike, SockaddrStorage},
};
use std::{
    net::{Ipv4Addr, ToSocketAddrs, UdpSocket},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

fn ipv4(address: Option<SockaddrStorage>) -> Option<Ipv4Addr> {
    address.and_then(|address| address.as_sockaddr_in().map(|address| address.ip()))
}

struct Interface {
    address: String,
    netmask: Option<String>,
    broadcast: Option<String>,
}

fn interfaces() -> Result<Vec<Interface>, Error> {
    let addresses: Vec<_> = getifaddrs()
        .map_err(|error| Error::typed("OSError", error.to_string()))?
        .collect();
    let mut grouped: Vec<(String, Vec<_>)> = Vec::new();
    let mut addresses: Vec<_> = addresses.into_iter().enumerate().collect();
    addresses.sort_by_key(|(index, address)| {
        (
            address
                .address
                .as_ref()
                .and_then(|address| address.family())
                .map(|family| family as i32)
                .unwrap_or(0),
            *index,
        )
    });
    for (_, address) in addresses {
        let name = address.interface_name.clone();
        if let Some((_, group)) = grouped.iter_mut().find(|(key, _)| key == &name) {
            group.push(address);
        } else {
            grouped.push((name, vec![address]));
        }
    }
    Ok(grouped
        .into_iter()
        .flat_map(|(_, group)| group)
        .filter_map(|address| {
            let ip = ipv4(address.address)?;
            if ip.octets()[0] == 127 {
                return None;
            }
            Some(Interface {
                address: ip.to_string(),
                netmask: ipv4(address.netmask).map(|ip| ip.to_string()),
                broadcast: ipv4(address.broadcast).map(|ip| ip.to_string()),
            })
        })
        .collect())
}

fn detected() -> String {
    if let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        if socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80)).is_ok() {
            if let Ok(address) = socket.local_addr() {
                return address.ip().to_string();
            }
        }
    }
    nix::unistd::gethostname()
        .ok()
        .and_then(|name| name.to_str().map(str::to_owned))
        .and_then(|name| (name.as_str(), 80).to_socket_addrs().ok())
        .and_then(|mut addresses| addresses.find(|address| address.is_ipv4()))
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".into())
}

pub fn targets(advertised: Option<&str>) -> Result<Vec<(String, String)>, Error> {
    let mut result = Vec::new();
    for Interface {
        address: ip,
        netmask,
        mut broadcast,
    } in interfaces()?
    {
        if advertised.is_some_and(|advertised| advertised != ip) {
            continue;
        }
        if broadcast.is_none() {
            broadcast = netmask
                .and_then(|mask| mask.parse::<Ipv4Addr>().ok())
                .and_then(|mask| {
                    let mask = u32::from(mask);
                    let host = !mask;
                    if host & host.wrapping_add(1) != 0 {
                        return None;
                    }
                    ip.parse::<Ipv4Addr>()
                        .ok()
                        .map(|ip| Ipv4Addr::from(u32::from(ip) | host).to_string())
                });
        }
        let target = (ip, broadcast.unwrap_or_else(|| "255.255.255.255".into()));
        if !result.contains(&target) {
            result.push(target);
        }
    }
    if result.is_empty() {
        let ip = advertised
            .filter(|ip| !ip.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(detected);
        if !ip.starts_with("127.") {
            result.push((ip, "255.255.255.255".into()));
        }
    }
    Ok(result)
}

fn broadcast(advertised: Option<&str>) -> Result<(), Error> {
    for (ip, destination) in targets(advertised)? {
        let body = Value::object([("ip", Value::text(&ip)), ("navi_debug", Value::integer(1))])
            .encode()?;
        if let Ok(socket) = UdpSocket::bind((ip.as_str(), 0)) {
            if socket.set_broadcast(true).is_ok() {
                let _ = socket.send_to(body.as_bytes(), (destination.as_str(), 7705));
            }
        }
    }
    Ok(())
}

pub struct Beacon {
    stop: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}
impl Beacon {
    pub fn start(advertised: Option<String>) -> Result<Self, Error> {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let flag = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("carrot_navi_discovery".into())
            .spawn(move || loop {
                if flag.0.lock().map_or(true, |stop| *stop) {
                    break;
                }
                let _ = broadcast(advertised.as_deref());
                let Ok(stop) = flag.0.lock() else {
                    break;
                };
                let Ok((stop, _)) =
                    flag.1
                        .wait_timeout_while(stop, Duration::from_secs(1), |stop| !*stop)
                else {
                    break;
                };
                if *stop {
                    break;
                }
            })
            .map_err(super::io)?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
    pub fn stop(&mut self) {
        if let Ok(mut stop) = self.stop.0.lock() {
            *stop = true;
            self.stop.1.notify_all();
        }
        let until = std::time::Instant::now() + Duration::from_secs(1);
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() && std::time::Instant::now() < until {
                thread::sleep(Duration::from_millis(1));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}
impl Drop for Beacon {
    fn drop(&mut self) {
        self.stop();
    }
}
