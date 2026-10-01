use crate::{
    bus::{self, Address},
    engine::Engine,
    get_security_type, Error, Event, MeteredType, Network, SecurityType,
};
use dbus::arg::{PropMap, Variant};
use std::time::Duration;

fn network(props: &PropMap, tethering: &str) -> Result<Network, Error> {
    let get = |name| props.get(name).ok_or(Error::Property(name));
    let ssid = String::from_utf8_lossy(&bus::bytes(get("Ssid")?, "Ssid")?).into_owned();
    let _bssid = bus::text(get("HwAddress")?, "HwAddress")?;
    let strength = i32::try_from(bus::unsigned(get("Strength")?, "Strength")?)
        .map_err(|_| Error::Property("Strength"))?;
    let flags = bus::unsigned(get("Flags")?, "Flags")?;
    let wpa = bus::unsigned(get("WpaFlags")?, "WpaFlags")?;
    let rsn = bus::unsigned(get("RsnFlags")?, "RsnFlags")?;
    Ok(Network {
        is_tethering: ssid == tethering,
        ssid,
        strength,
        security_type: get_security_type(flags, wpa, rsn),
    })
}
impl Engine {
    pub async fn update_networks(&self) -> Result<(), Error> {
        if !self.state()?.active {
            return Ok(());
        }
        let _scan = self.scan_lock.lock().await;
        let (device, tethering) = {
            let state = self.state()?;
            (state.device.clone(), state.tethering_ssid.clone())
        };
        let Some(device) = device else {
            self.warn("No WiFi device found".into());
            return Ok(());
        };
        let props = match self
            .main
            .properties(Address {
                path: &device,
                interface: bus::WIRELESS,
            })
            .await
        {
            Ok(props) => props,
            Err(Error::Reply(error)) => {
                self.warn(format!("Failed to get WiFi properties: {error}"));
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let paths = props
            .get("AccessPoints")
            .map(|v| bus::paths(v, "AccessPoints"))
            .transpose()?
            .unwrap_or_default();
        let mut networks: Vec<(Network, PropMap)> = Vec::new();
        for path in paths {
            let props = match self
                .main
                .properties(Address {
                    path: &path,
                    interface: bus::ACCESS_POINT,
                })
                .await
            {
                Ok(props) => props,
                Err(Error::Reply(_)) => {
                    self.warn(format!("Failed to get AP properties for {path}"));
                    continue;
                }
                Err(error) => return Err(error),
            };
            match network(&props, &tethering) {
                Ok(candidate) if !candidate.ssid.is_empty() => {
                    if let Some((old, old_props)) = networks
                        .iter_mut()
                        .find(|(old, _)| old.ssid == candidate.ssid)
                    {
                        if candidate.strength > old.strength {
                            *old = candidate;
                            *old_props = props;
                        }
                    } else {
                        networks.push((candidate, props));
                    }
                }
                Ok(_) => {}
                Err(error) => self.log(
                    openpilot_logging::record::Level::Error,
                    format!("Failed to parse AP properties for {path}: {error}"),
                ),
            }
        }
        for (network, props) in &mut networks {
            if network.security_type == SecurityType::Unsupported {
                let flags = bus::unsigned(&props["Flags"], "Flags")?;
                let wpa = bus::unsigned(&props["WpaFlags"], "WpaFlags")?;
                let rsn = bus::unsigned(&props["RsnFlags"], "RsnFlags")?;
                self.warn(format!(
                    "Unsupported network! flags: {flags}, wpa_flags: {wpa}, rsn_flags: {rsn}"
                ));
            }
            if network.is_tethering {
                network.strength = 100;
            }
        }
        self.state()?.snapshot.networks =
            networks.into_iter().map(|(network, _)| network).collect();
        self.active_info().await?;
        let mut state = self.state()?;
        let sorted = state.networks();
        state.events.push(Event::NetworksUpdated(sorted));
        Ok(())
    }
    pub async fn active_info(&self) -> Result<(), Error> {
        let mut ipv4 = String::new();
        let mut metered = MeteredType::Unknown;
        if let Some((path, props)) = self.active_connection(&self.main).await? {
            let ip_path = props
                .get("Ip4Config")
                .map(|v| bus::text(v, "Ip4Config"))
                .transpose()?
                .unwrap_or_else(|| "/".into());
            if ip_path != "/" {
                let (Variant(addresses),): (Variant<Vec<PropMap>>,) = self
                    .main
                    .call(
                        Address {
                            path: &ip_path,
                            interface: bus::PROPERTIES,
                        },
                        "Get",
                        (bus::IPV4, "AddressData"),
                    )
                    .await?;
                for address in addresses {
                    if let Some(value) = address.get("address") {
                        ipv4 = bus::text(value, "address")?;
                        break;
                    }
                }
            }
            let settings = self.connection_settings(&path).await?;
            if !settings.is_empty() {
                let group = settings
                    .get("connection")
                    .ok_or(Error::Property("connection"))?;
                metered = match group.get("metered").and_then(|v| v.0.as_i64()) {
                    Some(1) => MeteredType::Yes,
                    Some(2) => MeteredType::No,
                    _ => MeteredType::Unknown,
                };
            }
        }
        let mut state = self.state()?;
        state.snapshot.ipv4_address = ipv4;
        state.snapshot.current_network_metered = metered;
        Ok(())
    }
    pub async fn scan(&self) -> Result<(), Error> {
        loop {
            if self.state()?.scan_due(monotonic()?) {
                let device = self.state()?.device.clone();
                if let Some(device) = device {
                    match self
                        .main
                        .call::<_, ()>(
                            Address {
                                path: &device,
                                interface: bus::WIRELESS,
                            },
                            "RequestScan",
                            (PropMap::new(),),
                        )
                        .await
                    {
                        Ok(()) => {}
                        Err(Error::Reply(error)) => {
                            self.warn(format!("Failed to request scan: {error}"))
                        }
                        Err(error) => return Err(error),
                    }
                } else {
                    self.warn("No WiFi device found".into());
                }
                self.state()?.last_scan = monotonic()?;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}
fn monotonic() -> Result<f64, Error> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Ok(std::time::Duration::new(
        u64::try_from(time.tv_sec).map_err(|_| Error::ClockRange)?,
        u32::try_from(time.tv_nsec).map_err(|_| Error::ClockRange)?,
    )
    .as_secs_f64())
}
