use crate::helpers;
use std::net::UdpSocket;

#[allow(unsafe_code)]
#[cxx::bridge(namespace = "openpilot::cweb")]
mod ffi {
    // Synchronous ioctl adapter borrows the interface bytes only for this call and owns its socket.
    unsafe extern "C++" {
        include!("native/address.h");
        fn interface_ipv4(name: &[u8]) -> Result<String>;
    }
}
pub fn interface_ipv4(name: &str) -> String {
    let name: String = name.chars().take(15).collect();
    ffi::interface_ipv4(name.as_bytes())
        .map(|value| helpers::usable_ip(&value))
        .unwrap_or_default()
}
pub fn route_ipv4() -> String {
    let address = (|| -> std::io::Result<_> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(std::time::Duration::from_millis(500)))?;
        socket.connect("8.8.8.8:80")?;
        Ok(socket.local_addr()?.ip().to_string())
    })();
    address
        .map(|value| helpers::usable_ip(&value))
        .unwrap_or_default()
}
pub fn local_ip(iface: &str) -> String {
    let address = interface_ipv4(iface);
    if address.is_empty() {
        route_ipv4()
    } else {
        address
    }
}
