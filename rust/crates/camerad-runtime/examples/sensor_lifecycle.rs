use openpilot_camera_kernel::{Device, MemoryPool};
use openpilot_camerad::sensor::Register;
use openpilot_camerad_runtime::SensorPort;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let port: usize = args.get(1).ok_or("missing camera port")?.parse()?;
    let enabled = args.get(2).is_some_and(|value| value == "1");
    let request = Device::open("/dev/camera-fixture-request")?;
    let mut pool = MemoryPool::new(&request);
    let sensor = Device::open("/dev/camera-fixture-sensor")?;
    let mut camera = SensorPort::probe(&request, &pool, sensor, port, enabled)?;
    let opened = camera.acquired();
    camera.start()?;
    if opened.is_some() {
        camera.poke(i32::MIN + 1)?;
        camera.write_registers(&[Register(0x1234, 0x5678), Register(0x4321, 0x8765)])?;
    }
    println!(
        "{}",
        serde_json::json!({
            "enabled": camera.enabled(),
            "sensor": format!("{:?}", camera.kind()),
            "session": opened.map(|(session, _)| session.0),
            "device": opened.map(|(_, device)| device.0),
        })
    );
    camera.shutdown();
    drop(camera);
    pool.close()?;
    Ok(())
}
