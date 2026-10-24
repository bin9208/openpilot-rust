//! Dynamic, source-derived camera reference generators.
//! Algorithms and controlled-boundary substitutions are translated from the
//! original project generators pinned in camera-lifecycle-generators/original.json.
//! Current C++ and templates remain the policy input; no Python is executed.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    seal::Result,
};
use std::{fs, io::Write, path::Path};
#[derive(Clone, Copy)]
pub enum Kind {
    Ae,
    Misc,
    Kernel,
    SensorLifecycle,
    IspLifecycle,
    CameraLifecycle,
    State,
    Stress,
    RuntimeFixture,
}
fn read(root: &Path, path: &str) -> Result<String> {
    Ok(fs::read_to_string(root.join(path))?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn find(text: &str, needle: &str) -> Result<usize> {
    text.find(needle)
        .ok_or_else(|| format!("camera source missing {needle:?}").into())
}
fn between<'a>(text: &'a str, first: &str, last: &str) -> Result<&'a str> {
    let start = find(text, first)?;
    let end = start + find(&text[start..], last)?;
    Ok(&text[start..end])
}
/// Preserve the original deliberately naive brace counter, including C++ strings/comments.
pub fn extract<'a>(text: &'a str, signature: &str) -> Result<&'a str> {
    let start = find(text, signature)?;
    let body = start + find(&text[start..], "{")?;
    let mut depth = 1usize;
    for (offset, byte) in text.as_bytes()[body + 1..].iter().enumerate() {
        if *byte == b'{' {
            depth += 1;
        } else if *byte == b'}' {
            depth -= 1;
        }
        if depth == 0 {
            return Ok(&text[start..body + offset + 2]);
        }
    }
    Err(format!("camera unclosed source body {signature:?}").into())
}
// Some original slices find the end globally rather than after the start.
fn global_section<'a>(text: &'a str, first: &str, last: &str) -> Result<&'a str> {
    let start = find(text, first)?;
    let end = find(text, last)?;
    Ok(if end < start { "" } else { &text[start..end] })
}
fn line_method<'a>(text: &'a str, signature: &str, last: &str) -> Result<&'a str> {
    let start = find(text, signature)?;
    let end = start + find(&text[start..], last)? + last.len();
    Ok(&text[start..end])
}
fn once(text: String, before: &str, after: &str) -> Result<String> {
    if text.matches(before).count() != 1 {
        return Err(format!("camera replacement must occur once: {before:?}").into());
    }
    Ok(text.replace(before, after))
}
fn changes(mut text: String, pairs: &[(&str, &str)]) -> Result<String> {
    for (a, b) in pairs {
        text = once(text, a, b)?;
    }
    Ok(text)
}
fn split_two<'a>(text: &'a str, marker: &str) -> Result<(&'a str, &'a str)> {
    let mut parts = text.split(marker);
    let a = parts.next().unwrap();
    let b = parts.next().ok_or("camera template split missing")?;
    if parts.next().is_some() {
        return Err("camera template split has extra part".into());
    }
    Ok((a, b))
}
fn stripped(text: String) -> String {
    format!(
        "{}\n",
        text.trim_end_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
    )
}
const CAMERA: &str = "openpilot/system/camerad/cameras/";
fn camera(root: &Path, name: &str) -> Result<String> {
    read(root, &format!("{CAMERA}{name}"))
}
fn template(root: &Path, name: &str) -> Result<String> {
    read(root, &format!("rust/tools/{name}"))
}
fn ae(root: &Path, misc: bool) -> Result<String> {
    if misc {
        let source = camera(root, "camera_common.cc")?;
        let method = line_method(&source, "float calculate_exposure_value(", "\n}")?;
        return Ok(stripped(
            template(root, "camerad_misc_source.cc.in")?.replace("@LUMINANCE@", method),
        ));
    }
    let source = camera(root, "camera_qcom2.cc")?;
    let mut output = template(root, "camerad_ae_source.cc.in")?;
    let methods = [
        "set_exposure_rect",
        "update_exposure_score",
        "set_camera_exposure",
    ]
    .iter()
    .map(|n| line_method(&source, &format!("void CameraState::{n}("), "\n}"))
    .collect::<Result<Vec<_>>>()?;
    let gain = line_method(&source, "float get_gain_factor() const {", "\n  }")?;
    output = output.replace("@GAIN_FACTOR@", gain);
    let initialization = between(&source, "  fl_pix = camera.cc.focal_len", "\n\n  pm =")?;
    Ok(stripped(
        output
            .replace("@METHODS@", &methods.join("\n\n"))
            .replace("@INITIALIZE@", initialization),
    ))
}
const COMMON_METHODS: &[&str] = &[
    "int do_cam_control(",
    "std::optional<int32_t> device_acquire(",
    "int device_config(",
    "int device_control(",
    "void *alloc_w_mmu_hdl(",
    "void release(",
    "void *MemoryManager::alloc_buf(",
    "void MemoryManager::free(",
    "MemoryManager::~MemoryManager(",
];
fn helper_parts(
    source: &str,
    header: &str,
    util: &str,
    unique: bool,
    sync: bool,
) -> Result<Vec<String>> {
    let mut parts = vec![between(util, "#define HANDLE_EINTR(", "\n\n")?.into()];
    if unique {
        parts.push(format!("{};", extract(util, "struct unique_fd")?));
    }
    parts.push(
        global_section(
            header,
            "std::optional<int32_t> device_acquire(",
            "class MemoryManager",
        )?
        .into(),
    );
    parts.push(format!("{};", extract(header, "class MemoryManager")?));
    for (i, signature) in COMMON_METHODS.iter().enumerate() {
        parts.push(extract(source, signature)?.into());
        if sync && i == 0 {
            parts.push(extract(source, "int do_sync_control(")?.into());
        }
    }
    Ok(parts)
}
fn kernel(root: &Path) -> Result<String> {
    let source = camera(root, "spectra.cc")?;
    let header = camera(root, "spectra.h")?;
    let util = read(root, "openpilot/common/util.h")?;
    let macro_text = between(&util, "#define HANDLE_EINTR(", "\n\n")?;
    let declarations = global_section(
        &header,
        "std::optional<int32_t> device_acquire(",
        "class MemoryManager",
    )?;
    let mut methods = Vec::new();
    for (i, signature) in COMMON_METHODS.iter().enumerate() {
        methods.push(extract(&source, signature)?);
        if i == 0 {
            methods.push(extract(&source, "int do_sync_control(")?);
        }
    }
    let mut parts = vec![
        macro_text.to_owned(),
        declarations.to_owned(),
        format!("{};", extract(&header, "class MemoryManager")?),
    ];
    parts.extend(methods.iter().map(|s| s.to_string()));
    let mut output =
        template(root, "camerad_kernel_source.cc.in")?.replace("@HELPERS@", &parts.join("\n\n"));
    for (name, first, last) in [
        (
            "SESSION",
            "  struct cam_req_mgr_session_info session_info = {};",
            "\n  // access the sensor",
        ),
        (
            "LINK",
            "  struct cam_req_mgr_link_info req_mgr_link_info = {0};",
            "\n  struct cam_req_mgr_link_control req_mgr_link_control = {0};",
        ),
        (
            "SCHEDULE",
            "  struct cam_req_mgr_sched_request req_mgr_sched_request = {0};",
            "\n  // poke sensor",
        ),
        (
            "IMPORTS",
            "    struct cam_mem_mgr_map_cmd mem_mgr_map_cmd = {0};",
            "\n  }\n}\n\nbool SpectraCamera::openSensor",
        ),
        (
            "FENCES",
            "  struct cam_sync_info sync_create = {0};",
            "\n  // schedule request",
        ),
    ] {
        let fragment = between(&source, first, last)?;
        let fragment = if name == "FENCES" {
            fragment.replace(
                "  if (icp_dev_handle > 0)",
                "  int first_ret = ret, first_errno = errno;\n  if (icp_dev_handle > 0)",
            )
        } else {
            fragment.into()
        };
        output = output.replace(&format!("@{name}@"), &fragment);
    }
    Ok(stripped(output))
}
const SENSOR_METHODS: &[&str] = &[
    "static cam_cmd_power *power_set_wait(",
    "int SpectraCamera::sensors_init(",
    "void SpectraCamera::sensors_i2c(",
    "void SpectraCamera::sensors_poke(",
    "void SpectraCamera::sensors_start(",
    "bool SpectraCamera::openSensor(",
];
const ISP_METHODS: &[&str] = &[
    "void add_patch(",
    "int get_bps_blob_index(",
    "void SpectraCamera::configISP(",
    "void SpectraCamera::configICP(",
    "void SpectraCamera::config_ife(",
    "void SpectraCamera::config_bps(",
];
fn constants(header: &str) -> Result<&str> {
    line_method(header, "enum {", "\n};")
}
fn sensor(root: &Path, fixture: bool) -> Result<String> {
    if fixture {
        let output = changes(
            template(root, "camerad_kernel_fixture.cc")?,
            SENSOR_FIXTURE_CHANGES,
        )?;
        return Ok(format!(
            "{output}{}\n",
            template(root, "camerad_sensor_lifecycle_fixture.cc.in")?
        ));
    }
    let source = camera(root, "spectra.cc")?;
    let header = camera(root, "spectra.h")?;
    let util = read(root, "openpilot/common/util.h")?;
    let helpers = helper_parts(&source, &header, &util, true, false)?;
    let methods = SENSOR_METHODS
        .iter()
        .map(|s| extract(&source, s))
        .collect::<Result<Vec<_>>>()?;
    let constants = constants(&header)?;
    let output = template(root, "camerad_sensor_lifecycle_source.cc.in")?;
    Ok(stripped(
        output
            .replace("@CONSTANTS@", constants)
            .replace("@HELPERS@", &helpers.join("\n\n"))
            .replace("@METHODS@", &methods.join("\n\n")),
    ))
}
fn isp(root: &Path, fixture: bool) -> Result<String> {
    if fixture {
        let output = changes(
            template(root, "camerad_kernel_fixture.cc")?,
            ISP_FIXTURE_CHANGES,
        )?;
        let suffix = template(root, "camerad_isp_lifecycle_fixture.cc.in")?;
        let (prelude, wrapper) = split_two(&suffix, "// @WRAPPER@\n")?;
        let output = output.replacen(
            "extern \"C\" int munmap(",
            &format!("{prelude}\nextern \"C\" int munmap("),
            1,
        );
        return Ok(format!(
            "#include <algorithm>\n#include <cstddef>\n#include <cstdint>\n#include <media/cam_isp.h>\n#include <media/cam_icp.h>\n{output}{wrapper}\n"
        ));
    }
    let source = camera(root, "spectra.cc")?;
    let header = camera(root, "spectra.h")?;
    let util = read(root, "openpilot/common/util.h")?;
    let helpers = helper_parts(&source, &header, &util, true, false)?;
    let methods = ISP_METHODS
        .iter()
        .map(|s| extract(&source, s))
        .collect::<Result<Vec<_>>>()?;
    let ife = camera(root, "ife.h")?;
    let output = template(root, "camerad_isp_lifecycle_source.cc.in")?;
    Ok(stripped(
        output
            .replace("@HELPERS@", &helpers.join("\n\n"))
            .replace(
                "@BUFFER@",
                &format!("{};", extract(&header, "class SpectraBuf")?),
            )
            .replace("@IFE@", &ife[find(&ife, "int build_common_ife_bps(")?..])
            .replace("@METHODS@", &methods.join("\n\n")),
    ))
}
const SENSOR_FIXTURE_CHANGES: &[(&str, &str)] = &[
    (
        r#"extern "C" int ioctl(int fd, unsigned long request, ...) noexcept {"#,
        r#"static int kernel_ioctl(int fd, unsigned long request, ...) noexcept {"#,
    ),
    (
        r#"  if (fd < 0) {"#,
        r#"  if (!strcmp(path, "/dev/camera-fixture-sensor")) fd = 502;
  if (fd < 0) {"#,
    ),
    (r#"char bytes[16384]"#, r#"char bytes[262144]"#),
    (r#"uint8_t bytes[4096]"#, r#"uint8_t bytes[65536]"#),
    (r#"if (length > 4096)"#, r#"if (length > 65536)"#),
    (r#"char data[8193]"#, r#"char data[131073]"#),
];

const ISP_FIXTURE_CHANGES: &[(&str, &str)] = &[
    (
        r#"extern "C" int ioctl(int fd, unsigned long request, ...) noexcept {"#,
        r#"static int kernel_ioctl(int fd, unsigned long request, ...) noexcept {"#,
    ),
    (
        r#"  if (fd < 0) {"#,
        r#"  if (!strcmp(path, "/dev/camera-fixture-isp")) fd = 503;
  if (!strcmp(path, "/dev/camera-fixture-icp")) fd = 504;
  if (!strcmp(path, "/dev/camera-fixture-sync")) fd = 505;
  if (fd < 0) {"#,
    ),
    (
        r#"    if (length > 4096) abort();
    char data[8193]; hex(address, length, data);
    trace("{\"op\":\"munmap\",\"fd\":%d,\"length\":%zu,\"data\":\"%s\"}", fd, length, data);"#,
        r#"    dump_memory("release", fd, address, 0, length);
    trace("{\"op\":\"munmap\",\"fd\":%d,\"length\":%zu}", fd, length);"#,
    ),
    (
        r#"      if (command->len > sizeof(bytes) || syscall(SYS_pwrite64, export_fd, bytes, command->len, 0) != static_cast<ssize_t>(command->len)) abort();"#,
        r#"      for (size_t offset = 0; offset < command->len; offset += sizeof(bytes)) {
        size_t count = std::min(sizeof(bytes), static_cast<size_t>(command->len) - offset);
        if (syscall(SYS_pwrite64, export_fd, bytes, count, offset) != static_cast<ssize_t>(count)) abort();
      }"#,
    ),
];

const CAMERA_FIXTURE_CHANGES: &[(&str, &str)] = &[
    (
        r#"#include <media/cam_icp.h>"#,
        r#"#include <media/cam_icp.h>
#include <media/cam_sensor.h>
#include <sys/stat.h>"#,
    ),
    (
        r#"static int tracked[2048]"#,
        r#"static int camera_target = 501, camera_event = 0, camera_packet_opcode = -1;
static int tracked[2048]"#,
    ),
    (
        r#"  const char* wanted = getenv("CK_FAIL_OP");"#,
        r#"  if (setting("CK_FAIL_TARGET", 0) && setting("CK_FAIL_TARGET", 0) != camera_target) return false;
  if (getenv("CK_FAIL_PACKET") && setting("CK_FAIL_PACKET", 0) != camera_packet_opcode) return false;
  const char* wanted = getenv("CK_FAIL_OP");"#,
    ),
    (
        r#"  if (!strcmp(path, "/dev/camera-fixture-isp")) fd = 503;"#,
        r#"  if (!strcmp(path, "/dev/camera-fixture-sensor")) fd = 502;
  if (!strcmp(path, "/dev/camera-fixture-phy")) fd = 506;
  if (!strcmp(path, "/dev/camera-fixture-isp")) fd = 503;"#,
    ),
    (
        r#"    return syscall(SYS_openat, AT_FDCWD, path, flags, mode);"#,
        r#"    int result = syscall(SYS_openat, AT_FDCWD, path, flags, mode);
    if (result >= 0 && strstr(path, "/dev/shm/msgq_visionbuf_") == path) {
      if (result >= 2048 || tracked[result]) abort();
      tracked[result] = 3; trace("{\"op\":\"vision-open\",\"fd\":%d}", result);
    }
    return result;"#,
    ),
    (
        r#"  if (owned) {"#,
        r#"  if (fd >= 0 && fd < 2048 && tracked[fd] == 3) {
    trace("{\"op\":\"vision-mmap\",\"fd\":%d,\"length\":%zu,\"ok\":%s}", fd, length, p == MAP_FAILED ? "false" : "true");
    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; }
  }
  if (owned) {"#,
    ),
    (
        r#"extern "C" int munmap(void* address, size_t length) noexcept {"#,
        r#"extern "C" int munmap(void* address, size_t length) noexcept {
  for (int fd = 0; fd < 2048; ++fd) if (tracked[fd] == 3 && mappings[fd] == address && lengths[fd]) {
    trace("{\"op\":\"vision-munmap\",\"fd\":%d,\"length\":%zu}", fd, length);
    mappings[fd] = nullptr; lengths[fd] = 0; return syscall(SYS_munmap, address, length);
  }"#,
    ),
    (
        r#"if (fd != 501 && fd != 503 && fd != 504 && fd != 505)"#,
        r#"if (fd < 501 || fd > 506)"#,
    ),
    (
        r#"if (acquire) reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = -1003;"#,
        r#"if (acquire) reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = 1000 + camera_target;"#,
    ),
    (
        r#"if (packet->num_cmd_buf != 2) abort();"#,
        r#"if (packet->num_cmd_buf > 2) abort();"#,
    ),
    (
        r#"  trace("{\"op\":\"target\",\"fd\":%d}", fd);"#,
        r#"  camera_target = fd; camera_packet_opcode = -1;
  trace("{\"op\":\"target\",\"fd\":%d}", fd);"#,
    ),
    (
        r#"data->session_hdl = -17; data->u.frame_msg.link_hdl = -31;"#,
        r#"data->session_hdl = -1001; data->u.frame_msg.link_hdl = -1005;"#,
    ),
    (
        r#"data->u.frame_msg.request_id = UINT64_C(0xdeadbeef01234567);"#,
        r#"data->u.frame_msg.request_id = ++camera_event;"#,
    ),
    (
        r#"data->u.frame_msg.frame_id = UINT64_C(0xfedcba9876543210);"#,
        r#"data->u.frame_msg.frame_id = camera_event;"#,
    ),
    (
        r#"data->u.frame_msg.timestamp = UINT64_C(0x1122334455667788);"#,
        r#"data->u.frame_msg.timestamp = UINT64_C(1000000000) + UINT64_C(50000000) * (camera_event - 1);
      lifecycle_event(data, camera_event);"#,
    ),
    (
        r#"data->u.frame_msg.sof_status = 7;"#,
        r#"data->u.frame_msg.sof_status = 0;"#,
    ),
    (
        r#"      if (packet->num_cmd_buf > 2) abort();"#,
        r#"      camera_packet_opcode = packet->header.op_code;
      if (packet->num_cmd_buf > 2) abort();"#,
    ),
];

const STATE_CHANGES: &[(&str, &str)] = &[
    (
        r#"struct Frame { uint32_t frame_id = 0; };"#,
        r#"struct FrameMetadata { uint32_t frame_id = 0, request_id = 0; uint64_t timestamp_sof = 0, timestamp_eof = 0; float processing_time = 0; };
struct Yuv { const uint8_t *y = nullptr; };
struct Raw { const void *addr = nullptr; size_t len = 0; };"#,
    ),
    (r#"struct Buffer {"#, r#"struct CameraBuf {"#),
    (
        r#"  Frame cur_frame_data;"#,
        r#"  FrameMetadata cur_frame_data;
  Yuv *cur_yuv_buf = nullptr;
  Raw *cur_camera_buf = nullptr;
  void sendFrameToVipc() { actions.push_back("vision"); }"#,
    ),
    (r#"  Buffer buf;"#, r#"  CameraBuf buf;"#),
    (
        r#"struct Config { int camera_num; float focal_len; };"#,
        r#"enum { VISION_STREAM_ROAD = 0, VISION_STREAM_DRIVER = 1, VISION_STREAM_WIDE_ROAD = 2 };
struct Config { int camera_num; float focal_len; int stream_type; const char *publish_name;
  cereal::FrameData::Builder (cereal::Event::Builder::*init_camera_state)(); };"#,
    ),
    (
        r#"    writes.assign(data, data + count);"#,
        r#"    actions.push_back("registers");
    writes.assign(data, data + count);"#,
    ),
    (
        r#"  void update_exposure_score(float, int, int, float);"#,
        r#"  std::unique_ptr<PubMaster> pm = std::make_unique<PubMaster>();
  void sendState();
  void update_exposure_score(float, int, int, float);"#,
    ),
    (r#"  std::cout << "]}\n";"#, r#"  std::cout << "]}";"#),
];

const RUNTIME_CHANGES: &[(&str, &str)] = &[
    (
        r#"  if (fd < 0) {"#,
        r#"  int actual = runtime_open(path, flags);
  if (actual == -2) return -1;
  if (actual < -100) return -actual-100;
  if (actual >= 0) fd = actual;
  if (fd < 0) {"#,
    ),
    (
        r#"  int ret = setting("CK_POLL_RETURN", 1), error = 0;"#,
        r#"  runtime_poll();
  int ret = setting("CK_POLL_RETURN", 1), error = 0;"#,
    ),
    (
        r#"reinterpret_cast<cam_req_mgr_session_info*>(data)->session_hdl = -1001;"#,
        r#"reinterpret_cast<cam_req_mgr_session_info*>(data)->session_hdl = 10001 + runtime_port;"#,
    ),
    (
        r#"reinterpret_cast<cam_req_mgr_link_info*>(data)->link_hdl = -1005;"#,
        r#"reinterpret_cast<cam_req_mgr_link_info*>(data)->link_hdl = 20001 + runtime_port;"#,
    ),
    (
        r#"reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = 1000 + camera_target;"#,
        r#"reinterpret_cast<cam_acquire_dev_cmd*>(data)->dev_handle = 100000 + runtime_port * 1000 + camera_target;"#,
    ),
    (
        r#"if (fd < 501 || fd > 506)"#,
        r#"if (fd < 501 || fd > 522)"#,
    ),
    (
        r#"      int ret = expected && probe->expected_data != expected ? -1 : 0;"#,
        r#"      int ret = (setting("CK_MISSING_PORT", -1) == runtime_port || (expected && probe->expected_data != expected)) ? -1 : 0;"#,
    ),
    (
        r#"    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; }
  }
  if (owned)"#,
        r#"    if (p != MAP_FAILED) { mappings[fd] = p; lengths[fd] = length; memset(p, 64 + runtime_port * 32, length); }
  }
  if (owned)"#,
    ),
];

const RUNTIME_QUERIES: &str = r#"
  if (request == VIDIOC_SUBSCRIBE_EVENT) {
    trace("{\"op\":\"subscribe\",\"fd\":%d}", fd);
    return 0;
  }
  if (request == VIDIOC_CAM_CONTROL) {
    auto* control = static_cast<cam_control*>(payload);
    if (control->op_code == CAM_QUERY_CAP) {
      auto* query = reinterpret_cast<cam_query_cap_cmd*>(control->handle);
      if (fd == 503) {
        auto* caps = reinterpret_cast<cam_isp_query_cap_cmd*>(query->caps_handle);
        caps->device_iommu.non_secure = 0x102; caps->cdm_iommu.non_secure = 0x304;
      } else if (fd == 504) {
        auto* caps = reinterpret_cast<cam_icp_query_cap_cmd*>(query->caps_handle);
        caps->dev_iommu_handle.non_secure = 0x506;
      } else abort();
      trace("{\"op\":\"query\",\"fd\":%d}", fd);
      return 0;
    }
  }
"#;

const RUNTIME_REGISTERS: &str = r#"
      if (fd >= 510 && fd <= 512 && packet->num_cmd_buf == 1 && runtime_round > 0) {
        const auto* bytes = static_cast<const uint8_t*>(mappings[descriptors[0].mem_handle >> 16]) + descriptors[0].offset;
        const auto* writes = reinterpret_cast<const cam_cmd_i2c_random_wr*>(bytes);
        if (descriptors[0].length >= sizeof(i2c_rdwr_header) && writes->header.count < 64) {
          char payload[4096];
          hex(writes->random_wr_payload, writes->header.count * sizeof(i2c_random_wr_payload), payload);
          trace("{\"op\":\"runtime-registers\",\"port\":%d,\"round\":%d,\"count\":%u,\"payload\":\"%s\"}",
                fd - 510, runtime_round, writes->header.count, payload);
          static bool barrier_used = false;
          if (const char* gate = getenv("CK_REGISTER_GATE"); gate && !barrier_used && fd == 510) {
            barrier_used = true;
            char ready[4096]; snprintf(ready, sizeof(ready), "%s.ready", gate);
            int signal_fd = syscall(SYS_openat, AT_FDCWD, ready, O_WRONLY | O_CREAT, 0600);
            if (signal_fd < 0) abort();
            syscall(SYS_close, signal_fd);
            while (syscall(SYS_faccessat, AT_FDCWD, gate, F_OK) != 0) runtime_pause();
            trace("{\"op\":\"runtime-register-barrier-released\",\"port\":0,\"round\":%d}", runtime_round);
          }
        }
      }
"#;

const CAMERA_METHODS: &[&str] = &[
    "static cam_cmd_power *power_set_wait(",
    "void add_patch(",
    "int get_bps_blob_index(",
    "SpectraCamera::SpectraCamera(",
    "SpectraCamera::~SpectraCamera(",
    "int SpectraCamera::sensors_init(",
    "void SpectraCamera::sensors_i2c(",
    "void SpectraCamera::sensors_poke(",
    "void SpectraCamera::sensors_start(",
    "bool SpectraCamera::openSensor(",
    "void SpectraCamera::configISP(",
    "void SpectraCamera::configICP(",
    "void SpectraCamera::config_ife(",
    "void SpectraCamera::config_bps(",
    "void SpectraCamera::configCSIPHY(",
    "void SpectraCamera::linkDevices(",
    "void SpectraCamera::camera_open(",
    "void SpectraCamera::camera_close(",
    "void SpectraCamera::camera_map_bufs(",
    "int SpectraCamera::clear_req_queue(",
    "void SpectraCamera::enqueue_frame(",
    "void SpectraCamera::destroySyncObjectAt(",
    "bool SpectraCamera::handle_camera_event(",
    "bool SpectraCamera::validateEvent(",
    "void SpectraCamera::clearAndRequeue(",
    "bool SpectraCamera::waitForFrameReady(",
    "bool SpectraCamera::processFrame(",
    "bool SpectraCamera::syncFirstFrame(",
];
fn camera_fixture(root: &Path) -> Result<String> {
    let mut output = changes(isp(root, true)?, CAMERA_FIXTURE_CHANGES)?;
    let helpers = template(root, "camerad_camera_lifecycle_fixture.cc.in")?;
    let (event, ioctl) = split_two(&helpers, "// @IOCTL@\n")?;
    output = once(
        output,
        "static int kernel_ioctl(int fd, unsigned long request, ...) noexcept {",
        &format!(
            "{event}\nstatic int kernel_ioctl(int fd, unsigned long request, ...) noexcept {{"
        ),
    )?;
    once(
        output,
        "    if (control->op_code == CAM_ACQUIRE_DEV) {",
        &format!("{ioctl}\n    if (control->op_code == CAM_ACQUIRE_DEV) {{"),
    )
}
fn camera_source(root: &Path) -> Result<String> {
    let source = camera(root, "spectra.cc")?;
    let header = camera(root, "spectra.h")?;
    let common = camera(root, "camera_common.cc")?;
    let common_header = camera(root, "camera_common.h")?;
    let util = read(root, "openpilot/common/util.h")?;
    let helpers = helper_parts(&source, &header, &util, true, true)?;
    let methods = CAMERA_METHODS
        .iter()
        .map(|s| extract(&source, s))
        .collect::<Result<Vec<_>>>()?;
    let constants = constants(&header)?;
    let ife = camera(root, "ife.h")?;
    let replacements = vec![
        ("@CONSTANTS@", constants.to_owned()),
        ("@HELPERS@", helpers.join("\n\n")),
        (
            "@MASTER@",
            format!("{};", extract(&header, "class SpectraMaster")?),
        ),
        (
            "@BUFFER@",
            format!("{};", extract(&header, "class SpectraBuf")?),
        ),
        (
            "@IMAGE@",
            format!(
                "{} FrameMetadata;\n{};",
                extract(&common_header, "typedef struct FrameMetadata")?,
                extract(&common_header, "class CameraBuf {")?
            ),
        ),
        (
            "@CAMERA@",
            format!("{};", extract(&header, "class SpectraCamera {")?),
        ),
        (
            "@IFE@",
            ife[find(&ife, "int build_common_ife_bps(")?..].to_owned(),
        ),
        ("@METHODS@", methods.join("\n\n")),
        (
            "@IMAGE_METHODS@",
            [
                extract(&common, "void CameraBuf::init(")?,
                extract(&common, "CameraBuf::~CameraBuf(")?,
            ]
            .join("\n\n"),
        ),
    ];
    let mut output = template(root, "camerad_camera_lifecycle_source.cc.in")?;
    for (marker, value) in replacements {
        output = once(output, marker, &value)?;
    }
    Ok(output)
}
fn state(root: &Path) -> Result<String> {
    let source = camera(root, "camera_qcom2.cc")?;
    let common = camera(root, "camera_common.cc")?;
    let generated = ae(root, false)?;
    let generated = generated[..find(&generated, "int main()")?].to_owned();
    let generated = changes(generated, STATE_CHANGES)?;
    let util = read(root, "openpilot/common/util.h")?;
    let transport = read(root, "openpilot/cereal/messaging/messaging.h")?;
    let output = template(root, "camerad_state_source.cc.in")?;
    let (prelude, main) = split_two(&output, "// @MAIN@\n")?;
    let prelude = prelude
        .replace(
            "@INIT_EVENT@",
            extract(&transport, "cereal::Event::Builder initEvent(")?,
        )
        .replace("@MAP_VALUE@", extract(&util, "T map_val(")?);
    let methods = [
        extract(&common, "kj::Array<uint8_t> get_raw_frame_image(")?,
        extract(&common, "float calculate_exposure_value(")?,
        extract(&source, "void CameraState::sendState(")?,
    ]
    .join("\n");
    Ok(format!("{prelude}{generated}\n{methods}\n{main}\n"))
}
fn runtime(root: &Path) -> Result<String> {
    let output = camera_fixture(root)?;
    let helpers = template(root, "camerad_runtime_fixture.cc.in")?;
    let output = once(
        output,
        "static int opened(const char* path, int flags, mode_t mode) {",
        &format!("{helpers}\nstatic int opened(const char* path, int flags, mode_t mode) {{"),
    )?;
    let output = changes(output, &RUNTIME_CHANGES[..2])?;
    let start = find(&output, "      data->session_hdl = -1001;")?;
    let marker = "      data->u.frame_msg.sof_status = 0;";
    let end = start + find(&output[start..], marker)? + marker.len();
    let output = format!(
        "{}      runtime_event(data);{}",
        &output[..start],
        &output[end..]
    );
    let output = changes(output, &RUNTIME_CHANGES[2..])?;
    let marker = "  camera_target = fd; camera_packet_opcode = -1;";
    let output = once(output, marker, &format!("{RUNTIME_QUERIES}\n{marker}"))?;
    let marker = "      for (uint32_t index = 0; index < packet->num_cmd_buf; ++index) snapshot(descriptors[index].mem_handle, descriptors[index].offset, descriptors[index].length);";
    if output.matches(marker).count() != 2 {
        return Err("camera runtime descriptor marker must occur twice".into());
    }
    let at = output.rfind(marker).unwrap() + marker.len();
    Ok(format!(
        "{}\n{RUNTIME_REGISTERS}{}",
        &output[..at],
        &output[at..]
    ))
}
pub fn generate(root: &Path, kind: Kind, fixture: bool) -> Result<String> {
    match kind {
        Kind::Ae => ae(root, false),
        Kind::Misc => ae(root, true),
        Kind::Kernel => kernel(root),
        Kind::SensorLifecycle => sensor(root, fixture),
        Kind::IspLifecycle => isp(root, fixture),
        Kind::CameraLifecycle => Ok(format!(
            "{}\n",
            if fixture {
                camera_fixture(root)?
            } else {
                camera_source(root)?
            }
        )),
        Kind::State => state(root),
        Kind::Stress => {
            let header = camera(root, "spectra.h")?;
            let method = extract(&header, "inline bool stress_test(")?;
            let output = template(root, "camerad_stress_source.cc.in")?;
            Ok(format!("{}\n", once(output, "@METHOD@", method)?))
        }
        Kind::RuntimeFixture => Ok(format!("{}\n", runtime(root)?)),
    }
}
pub fn main_for(mut kind: Kind) {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut specs = vec![OptionSpec::value("--source", true)];
    if matches!(kind, Kind::Ae) {
        specs.push(OptionSpec::append("--kind"));
    }
    if matches!(
        kind,
        Kind::SensorLifecycle | Kind::IspLifecycle | Kind::CameraLifecycle
    ) {
        specs.push(OptionSpec::flag("--fixture", None));
    }
    let parsed = match scan_os(&args, &specs) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Some(kinds) = parsed.append_values.get("--kind") {
        for value in kinds {
            if value == "ae" {
                kind = Kind::Ae;
            } else if value == "misc" {
                kind = Kind::Misc;
            } else {
                eprintln!("--kind must be ae or misc");
                std::process::exit(2);
            }
        }
    }
    if parsed.help {
        println!("Usage: generate-camerad-reference --source PATH");
        return;
    }
    let result = generate(
        Path::new(&parsed.values["--source"]),
        kind,
        parsed.values.contains_key("--fixture"),
    )
    .and_then(|text| Ok(std::io::stdout().lock().write_all(text.as_bytes())?));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
