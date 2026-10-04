use openpilot_camera_kernel::{CallResult, Device, Master, MemoryPool, MmuHandles};
use openpilot_camerad::{requests::StressPoint, startup::FrameSync};
use openpilot_camerad_runtime::{CameraConfig, CameraPort, FrameClock, OutputMode};
use openpilot_msgq::{VisionServer, VisionStream};

struct Clock;
impl FrameClock for Clock {
    fn now_ms(&mut self) -> f64 {
        1000.0
    }
    fn now_ns(&mut self) -> u64 {
        1_000_000_000
    }
    fn stress(&mut self, _: StressPoint) -> Result<bool, openpilot_camerad_runtime::CameraError> {
        Ok(false)
    }
    fn sleep_ms(&mut self, _: u64) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mode = match args.get(1).map(String::as_str) {
        Some("0") => OutputMode::Raw,
        Some("1") => OutputMode::Ife,
        Some("2") => OutputMode::Bps,
        _ => return Err("invalid output mode".into()),
    };
    let depth = args.get(2).ok_or("missing depth")?.parse()?;
    let count: usize = args.get(3).map_or(Ok(3), |value| value.parse())?;
    let master = Master {
        request: Device::open("/dev/camera-fixture-request")?,
        isp: Device::open("/dev/camera-fixture-isp")?,
        icp: Device::open("/dev/camera-fixture-icp")?,
        sync: Device::open("/dev/camera-fixture-sync")?,
        mmu: MmuHandles {
            device: 0x102,
            cdm: 0x304,
            icp: 0x506,
        },
        subscription: CallResult { code: 0, errno: 0 },
    };
    let mut pool = MemoryPool::new(&master.request);
    let server = VisionServer::new("camerad-lifecycle")?;
    let mut clock = Clock;
    let mut camera = CameraPort::open_with(
        &master,
        &pool,
        &server,
        CameraConfig {
            port: 0,
            enabled: std::env::var("CK_ENABLED").as_deref() != Ok("0"),
            mode,
            phy: 0x4001,
            vignetting: true,
            depth,
            stream: VisionStream::Road,
            staggered: false,
        },
        &mut clock,
        |name, _| {
            Device::open(if name == "cam-sensor-driver" {
                "/dev/camera-fixture-sensor"
            } else {
                "/dev/camera-fixture-phy"
            })
        },
    )?;
    camera.start_sensors()?;
    let mut sync = FrameSync::new(1);
    let mut events = Vec::new();
    for _ in 0..count {
        let (call, event) = master.request.dequeue_event();
        call.require_success(0x100)?;
        let result = camera.handle_event(event.frame, &mut sync, &mut clock)?;
        let frame = result.and_then(|value| value.frame);
        events.push(serde_json::json!({"accepted":frame.is_some(), "frame":frame}));
    }
    println!(
        "{}",
        serde_json::json!({"enabled":camera.enabled(), "kind":camera.kind() as u32,
        "session":camera.session().map(|value| value.0), "events":events})
    );
    camera.shutdown();
    drop(camera);
    drop(server);
    pool.close()?;
    Ok(())
}
