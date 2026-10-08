import argparse
from pathlib import Path
import subprocess
import sys

from generate_camerad_kernel_reference import extract


def main() -> None:
  parser = argparse.ArgumentParser(description="Extract original sendState and exposure paths with an observable transport boundary")
  parser.add_argument("--source", type=Path, required=True)
  args = parser.parse_args()
  root = args.source
  source = (root / "openpilot/system/camerad/cameras/camera_qcom2.cc").read_text()
  common = (root / "openpilot/system/camerad/cameras/camera_common.cc").read_text()
  generated = subprocess.check_output([sys.executable, str(root / "rust/tools/generate_camerad_ae_reference.py"), "--source", str(root)], text=True)
  generated = generated[: generated.index("int main()")]
  substitutions = {
    'struct Frame { uint32_t frame_id = 0; };': (
      'struct FrameMetadata { uint32_t frame_id = 0, request_id = 0; '
      + '''uint64_t timestamp_sof = 0, timestamp_eof = 0; float processing_time = 0; };
struct Yuv { const uint8_t *y = nullptr; };
struct Raw { const void *addr = nullptr; size_t len = 0; };'''
    ),
    'struct Buffer {': 'struct CameraBuf {',
    '  Frame cur_frame_data;': '''  FrameMetadata cur_frame_data;
  Yuv *cur_yuv_buf = nullptr;
  Raw *cur_camera_buf = nullptr;
  void sendFrameToVipc() { actions.push_back("vision"); }''',
    '  Buffer buf;': '  CameraBuf buf;',
    'struct Config { int camera_num; float focal_len; };': '''enum { VISION_STREAM_ROAD = 0, VISION_STREAM_DRIVER = 1, VISION_STREAM_WIDE_ROAD = 2 };
struct Config { int camera_num; float focal_len; int stream_type; const char *publish_name;
  cereal::FrameData::Builder (cereal::Event::Builder::*init_camera_state)(); };''',
    '    writes.assign(data, data + count);': '    actions.push_back("registers");\n    writes.assign(data, data + count);',
    '  void update_exposure_score(float, int, int, float);': (
      '  std::unique_ptr<PubMaster> pm = std::make_unique<PubMaster>();\n' + '  void sendState();\n  void update_exposure_score(float, int, int, float);'
    ),
    '  std::cout << "]}\\n";': '  std::cout << "]}";',
  }
  for before, after in substitutions.items():
    assert generated.count(before) == 1, before
    generated = generated.replace(before, after)
  util = (root / "openpilot/common/util.h").read_text()
  transport = (root / "openpilot/cereal/messaging/messaging.h").read_text()
  prelude, main = (root / "rust/tools/camerad_state_source.cc.in").read_text().split("// @MAIN@\n")
  prelude = prelude.replace("@INIT_EVENT@", extract(transport, "cereal::Event::Builder initEvent("))
  prelude = prelude.replace("@MAP_VALUE@", extract(util, "T map_val("))
  print(
    prelude
    + generated
    + "\n"
    + "\n".join(
      [
        extract(common, "kj::Array<uint8_t> get_raw_frame_image("),
        extract(common, "float calculate_exposure_value("),
        extract(source, "void CameraState::sendState("),
      ]
    )
    + "\n"
    + main
  )


if __name__ == "__main__":
  main()
