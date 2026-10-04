use super::{Buffer, CallResult, Device, Error};
use openpilot_camerad::ioctl::SUBSCRIBE_EVENT_IOCTL;

#[derive(Clone, Copy, Debug)]
pub struct MmuHandles {
    pub device: i32,
    pub cdm: i32,
    pub icp: i32,
}

#[derive(Debug)]
pub struct Master {
    pub icp: Device,
    pub isp: Device,
    pub sync: Device,
    pub request: Device,
    pub mmu: MmuHandles,
    pub subscription: CallResult,
}

impl Master {
    pub fn open() -> Result<Self, Error> {
        super::diagnostics::message(false, "-- Opening devices".into());
        let request = Device::open("/dev/v4l/by-path/platform-soc:qcom_cam-req-mgr-video-index0")?;
        super::diagnostics::message(true, "opened video0".into());
        let sync = Device::open("/dev/v4l/by-path/platform-cam_sync-video-index0")?;
        super::diagnostics::message(true, "opened video1 (cam_sync)".into());
        let isp = Device::discover("cam-isp", 0)?;
        super::diagnostics::message(true, format!("opened isp {}", isp.raw_fd()));
        let icp = Device::discover("cam-icp", 0)?;
        super::diagnostics::message(true, format!("opened icp {}", icp.raw_fd()));
        super::diagnostics::message(false, "-- Query for MMU handles".into());
        let (device, cdm) = isp.query_isp()?;
        super::diagnostics::message(true, format!("using MMU handle: {:x}", device as u32));
        super::diagnostics::message(true, format!("using MMU handle: {:x}", cdm as u32));
        let icp_mmu = icp.query_icp()?;
        super::diagnostics::message(true, format!("using ICP MMU handle: {:x}", icp_mmu as u32));
        super::diagnostics::message(false, "-- Subscribing".into());
        let mut subscription = Buffer::<32>::zeroed();
        subscription.put32(0, 0x0800_0000);
        subscription.put32(4, 2);
        // SAFETY: FFI: the subscription ABI has 32 initialized bytes and contains no pointers.
        let result = unsafe { request.ioctl(SUBSCRIBE_EVENT_IOCTL, &mut subscription) };
        super::diagnostics::message(true, format!("req mgr subscribe: {}", result.code));
        Ok(Self {
            request,
            sync,
            isp,
            icp,
            mmu: MmuHandles {
                device,
                cdm,
                icp: icp_mmu,
            },
            subscription: CallResult {
                code: result.code,
                errno: result.errno,
            },
        })
    }
}
