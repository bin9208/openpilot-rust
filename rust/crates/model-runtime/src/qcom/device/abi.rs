#[repr(C)]
#[derive(Default)]
pub(super) struct DevInfo {
    pub device_id: u32,
    pub chip_id: u32,
    pub mmu_enabled: u32,
    pub padding0: u32,
    pub gmem_gpubaseaddr: u64,
    pub gpu_id: u32,
    pub padding1: u32,
    pub gmem_sizebytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct Property {
    pub kind: u32,
    pub padding: u32,
    pub value: u64,
    pub sizebytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct WaitTimestamp {
    pub context_id: u32,
    pub timestamp: u32,
    pub timeout: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct CreateContext {
    pub flags: u32,
    pub drawctxt_id: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct DestroyContext {
    pub drawctxt_id: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct Allocate {
    pub size: u64,
    pub flags: u64,
    pub va_len: u64,
    pub mmapsize: u64,
    pub id: u32,
    pub metadata_len: u32,
    pub metadata: u64,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct Free {
    pub flags: u64,
    pub private: u64,
    pub id: u32,
    pub kind: u32,
    pub len: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct CommandObject {
    pub offset: u64,
    pub gpuaddr: u64,
    pub size: u64,
    pub flags: u32,
    pub id: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct GpuCommand {
    pub flags: u64,
    pub cmdlist: u64,
    pub cmdsize: u32,
    pub numcmds: u32,
    pub objlist: u64,
    pub objsize: u32,
    pub numobjs: u32,
    pub synclist: u64,
    pub syncsize: u32,
    pub numsyncs: u32,
    pub context_id: u32,
    pub timestamp: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct PowerConstraint {
    pub kind: u32,
    pub context_id: u32,
    pub data: u64,
    pub size: u64,
}

pub(super) trait Payload {}
impl Payload for Property {}
impl Payload for WaitTimestamp {}
impl Payload for CreateContext {}
impl Payload for DestroyContext {}
impl Payload for Allocate {}
impl Payload for Free {}
impl Payload for GpuCommand {}

const fn request<T>(direction: u32, number: u32) -> libc::c_ulong {
    ((direction << 30) | ((std::mem::size_of::<T>() as u32) << 16) | (9 << 8) | number)
        as libc::c_ulong
}

pub(super) const GET_PROPERTY: libc::c_ulong = request::<Property>(3, 0x02);
pub(super) const WAIT_TIMESTAMP: libc::c_ulong = request::<WaitTimestamp>(1, 0x07);
pub(super) const CREATE_CONTEXT: libc::c_ulong = request::<CreateContext>(3, 0x13);
pub(super) const DESTROY_CONTEXT: libc::c_ulong = request::<DestroyContext>(1, 0x14);
pub(super) const SET_PROPERTY: libc::c_ulong = request::<Property>(1, 0x32);
pub(super) const ALLOCATE: libc::c_ulong = request::<Allocate>(3, 0x45);
pub(super) const FREE: libc::c_ulong = request::<Free>(1, 0x46);
pub(super) const GPU_COMMAND: libc::c_ulong = request::<GpuCommand>(3, 0x4a);

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn matches_original_kgsl_abi() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/qcom/kgsl.json")).unwrap();
        macro_rules! check {
            ($typ:ty, $name:literal, $( $field:ident => $key:literal ),+ $(,)?) => {{
                assert_eq!(oracle["structs"][$name]["size"], size_of::<$typ>());
                $(assert_eq!(oracle["structs"][$name]["fields"][$key], offset_of!($typ, $field), "{} {}", $name, $key);)+
            }};
        }
        check!(DevInfo, "devinfo", device_id => "device_id", chip_id => "chip_id", mmu_enabled => "mmu_enabled",
            gmem_gpubaseaddr => "gmem_gpubaseaddr", gpu_id => "gpu_id", gmem_sizebytes => "gmem_sizebytes");
        check!(Property, "device_getproperty", kind => "type", value => "value", sizebytes => "sizebytes");
        check!(WaitTimestamp, "device_waittimestamp_ctxtid", context_id => "context_id", timestamp => "timestamp", timeout => "timeout");
        check!(CreateContext, "drawctxt_create", flags => "flags", drawctxt_id => "drawctxt_id");
        check!(DestroyContext, "drawctxt_destroy", drawctxt_id => "drawctxt_id");
        check!(Allocate, "gpuobj_alloc", size => "size", flags => "flags", va_len => "va_len", mmapsize => "mmapsize",
            id => "id", metadata_len => "metadata_len", metadata => "metadata");
        check!(Free, "gpuobj_free", flags => "flags", private => "priv", id => "id", kind => "type", len => "len");
        check!(CommandObject, "command_object", offset => "offset", gpuaddr => "gpuaddr", size => "size", flags => "flags", id => "id");
        check!(GpuCommand, "gpu_command", flags => "flags", cmdlist => "cmdlist", cmdsize => "cmdsize", numcmds => "numcmds",
            objlist => "objlist", objsize => "objsize", numobjs => "numobjs", synclist => "synclist", syncsize => "syncsize",
            numsyncs => "numsyncs", context_id => "context_id", timestamp => "timestamp");
        for (name, value) in [
            ("DEVICE_GETPROPERTY", GET_PROPERTY),
            ("DEVICE_WAITTIMESTAMP_CTXTID", WAIT_TIMESTAMP),
            ("DRAWCTXT_CREATE", CREATE_CONTEXT),
            ("DRAWCTXT_DESTROY", DESTROY_CONTEXT),
            ("SETPROPERTY", SET_PROPERTY),
            ("GPUOBJ_ALLOC", ALLOCATE),
            ("GPUOBJ_FREE", FREE),
            ("GPU_COMMAND", GPU_COMMAND),
        ] {
            assert_eq!(oracle["ioctls"][name], value, "{name}");
        }
    }
}
