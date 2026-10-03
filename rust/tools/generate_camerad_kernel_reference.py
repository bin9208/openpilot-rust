import argparse
from pathlib import Path


def extract(text: str, signature: str) -> str:
  start = text.index(signature)
  body = text.index("{", start)
  depth = 1
  end = body + 1
  while depth:
    depth += (text[end] == "{") - (text[end] == "}")
    end += 1
  return text[start:end]


def main() -> None:
  parser = argparse.ArgumentParser(description="Extract original camera kernel helpers and operation payloads")
  parser.add_argument("--source", type=Path, required=True)
  args = parser.parse_args()
  source = (args.source / "openpilot/system/camerad/cameras/spectra.cc").read_text()
  header = (args.source / "openpilot/system/camerad/cameras/spectra.h").read_text()
  util = (args.source / "openpilot/common/util.h").read_text()
  macro_start = util.index("#define HANDLE_EINTR(")
  macro = util[macro_start : util.index("\n\n", macro_start)]
  declarations = header[header.index("std::optional<int32_t> device_acquire(") : header.index("class MemoryManager")]
  methods = [
    extract(source, signature)
    for signature in (
      "int do_cam_control(",
      "int do_sync_control(",
      "std::optional<int32_t> device_acquire(",
      "int device_config(",
      "int device_control(",
      "void *alloc_w_mmu_hdl(",
      "void release(",
      "void *MemoryManager::alloc_buf(",
      "void MemoryManager::free(",
      "MemoryManager::~MemoryManager(",
    )
  ]
  fragments = {
    "SESSION": ("  struct cam_req_mgr_session_info session_info = {};", "\n  // access the sensor"),
    "LINK": ("  struct cam_req_mgr_link_info req_mgr_link_info = {0};", "\n  struct cam_req_mgr_link_control req_mgr_link_control = {0};"),
    "SCHEDULE": ("  struct cam_req_mgr_sched_request req_mgr_sched_request = {0};", "\n  // poke sensor"),
    "IMPORTS": ("    struct cam_mem_mgr_map_cmd mem_mgr_map_cmd = {0};", "\n  }\n}\n\nbool SpectraCamera::openSensor"),
    "FENCES": ("  struct cam_sync_info sync_create = {0};", "\n  // schedule request"),
  }
  template = (args.source / "rust/tools/camerad_kernel_source.cc.in").read_text()
  template = template.replace("@HELPERS@", "\n\n".join([macro, declarations, extract(header, "class MemoryManager") + ";", *methods]))
  for name, (start, end) in fragments.items():
    begin = source.index(start)
    fragment = source[begin : source.index(end, begin)]
    if name == "FENCES":
      fragment = fragment.replace("  if (icp_dev_handle > 0)", "  int first_ret = ret, first_errno = errno;\n  if (icp_dev_handle > 0)")
    template = template.replace(f"@{name}@", fragment)
  print(template.rstrip())


if __name__ == "__main__":
  main()
