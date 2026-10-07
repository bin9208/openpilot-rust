use openpilot_camera_kernel::{CallResult, Device, Master, MemoryPool, MmuHandles, Session};
use openpilot_camerad::{isp::FrameBuffers, sensor::SensorKind};
use openpilot_camerad_runtime::{IspConfig, IspPort, OutputMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let kind = match args.get(1).map(String::as_str) {
        Some("1") => SensorKind::Ar0231,
        Some("2") => SensorKind::Ox03c10,
        Some("3") => SensorKind::Os04c10,
        _ => return Err("invalid sensor".into()),
    };
    let mode = match args.get(2).map(String::as_str) {
        Some("0") => OutputMode::Raw,
        Some("1") => OutputMode::Ife,
        Some("2") => OutputMode::Bps,
        _ => return Err("invalid output mode".into()),
    };
    let depth = args.get(3).ok_or("missing depth")?.parse()?;
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
    let mut isp = IspPort::new(
        &master,
        &pool,
        Session(12),
        kind,
        IspConfig {
            camera: 0,
            mode,
            phy: 0x4001,
            vignetting: true,
            depth,
        },
    )?;
    for (slot, request) in [(0, 1), (depth - 1, i32::MIN + 1), (0, i32::MAX)] {
        isp.configure(
            slot,
            request,
            FrameBuffers {
                raw: 200 + slot as i32,
                yuv: 300 + slot as i32,
                ife_fence: 400 + slot as i32,
                bps_fence: 500 + slot as i32,
            },
        )?;
    }
    println!(
        "{}",
        serde_json::json!({"isp": isp.ife_handle().0, "bps": isp.bps_handle().map(|value| value.0)})
    );
    drop(isp);
    pool.close()?;
    Ok(())
}
