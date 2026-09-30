use std::{
    net::UdpSocket,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Default, Debug, serde::Serialize)]
pub struct IpState {
    pub ip: Option<String>,
    pub failures: u64,
}
impl IpState {
    pub fn refresh(&mut self, ip: Option<String>) {
        if let Some(ip) = ip {
            self.failures = 0;
            self.ip = Some(ip);
        } else {
            self.failures = self.failures.saturating_add(1);
            if self.failures >= 3 {
                self.ip = None;
            }
        }
    }
    pub fn label(&self, port: u16) -> String {
        self.ip
            .as_ref()
            .filter(|ip| !ip.is_empty())
            .map_or_else(|| "no network".into(), |ip| format!("{ip}:{port}"))
    }
}
pub fn query_ip() -> std::io::Result<String> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_millis(300)))?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip().to_string())
}
static MONITOR: Mutex<Option<Arc<Mutex<IpState>>>> = Mutex::new(None);
pub fn start() -> std::io::Result<Arc<Mutex<IpState>>> {
    let mut monitor = MONITOR
        .lock()
        .map_err(|_| std::io::Error::other("IP monitor initialization poisoned"))?;
    if let Some(state) = monitor.as_ref() {
        return Ok(state.clone());
    }
    let state = Arc::new(Mutex::new(IpState {
        ip: query_ip().ok(),
        ..IpState::default()
    }));
    let worker = state.clone();
    let handle = std::thread::Builder::new()
        .name("ip_monitor".into())
        .spawn(move || loop {
            let value = query_ip().ok();
            match worker.lock() {
                Ok(mut state) => state.refresh(value),
                Err(error) => {
                    eprintln!("ip_monitor lock: {error}");
                    return;
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        })?;
    drop(handle);
    *monitor = Some(state.clone());
    Ok(state)
}
