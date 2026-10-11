#pragma once
#include <memory>
#include <opencv2/videoio.hpp>
#include "rust/cxx.h"
namespace openpilot_webcam {
struct Info;
struct Frame;
class Capture {
public:
  cv::VideoCapture value;
};
std::unique_ptr<Capture> open_path(rust::Str path);
std::unique_ptr<Capture> open_index(std::int32_t index);
Info info(const Capture&);
bool opened(const Capture&);
Frame read(Capture&);
}
