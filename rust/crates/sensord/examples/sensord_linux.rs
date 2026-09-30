use openpilot_sensord::{
    linux::{realtime, Gpio, LinuxBus},
    loops::Poll,
    Error,
};
use serde_json::json;
use std::path::PathBuf;
fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let bus_path = PathBuf::from(args.next().ok_or(Error::Contract("bus path"))?);
    let gpio_path = PathBuf::from(args.next().ok_or(Error::Contract("GPIO path"))?);
    realtime(false)?;
    let mut bus = LinuxBus::open(&bus_path)?;
    let mut rows = Vec::new();
    rows.push(json!(bus.read_byte(0x6a, 0x0f, false)?));
    bus.write_byte(0x6a, 0x60, 255, true)?;
    rows.push(json!(bus.read_byte(0x6a, 0x60, false)?));
    for (reg, len) in [
        (0x40, 6),
        (0x41, 6),
        (0x42, 6),
        (0x43, 32),
        (0x44, 0),
        (0x44, 33),
        (0xee, 6),
    ] {
        rows.push(match bus.read_block(0x6a, reg, len, false) {
            Ok(v) => json!({"bytes":v}),
            Err(_) => json!({"error":true}),
        });
    }
    let mut gpio = Gpio::open(&gpio_path, "sensord", 84)?;
    let poll = gpio.poll(100)?;
    rows.push(match poll {
        Poll::Data(bytes) => json!({"gpio":bytes}),
        _ => return Err(Error::Contract("GPIO event missing")),
    });
    drop(gpio);
    drop(bus);
    serde_json::to_writer(std::io::stdout(), &rows)?;
    Ok(())
}
