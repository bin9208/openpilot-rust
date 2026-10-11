#include "openpilot-webcam/src/capture/bridge.rs.h"
#include <opencv2/core/version.hpp>
#include <limits>
#include <stdexcept>
static_assert(CV_VERSION_MAJOR == 4 && CV_VERSION_MINOR == 13 && CV_VERSION_REVISION == 0,
              "pinned OpenCV 4.13.0 required");
namespace openpilot_webcam {
static void requests(Capture& capture) {
  capture.value.set(cv::CAP_PROP_FRAME_WIDTH, 1280.0);
  capture.value.set(cv::CAP_PROP_FRAME_HEIGHT, 720.0);
  capture.value.set(cv::CAP_PROP_FPS, 25.0);
}
std::unique_ptr<Capture> open_path(rust::Str path) {
  auto capture = std::make_unique<Capture>();
  capture->value.open(std::string(path));
  requests(*capture);
  return capture;
}
std::unique_ptr<Capture> open_index(std::int32_t index) {
  auto capture = std::make_unique<Capture>();
  capture->value.open(index);
  requests(*capture);
  return capture;
}
Info info(const Capture& capture) {
  return Info{capture.value.get(cv::CAP_PROP_FRAME_WIDTH),
              capture.value.get(cv::CAP_PROP_FRAME_HEIGHT), capture.value.get(cv::CAP_PROP_FPS)};
}
bool opened(const Capture& capture) { return capture.value.isOpened(); }
Frame read(Capture& capture) {
  cv::Mat image;
  if (!capture.value.read(image)) {
    capture.value.release();
    return Frame{};
  }
  if (image.type() != CV_8UC3 || image.rows <= 0 || image.cols <= 0)
    throw std::invalid_argument("capture did not return a nonempty BGR24 image");
  const auto width = static_cast<std::size_t>(image.cols);
  const auto height = static_cast<std::size_t>(image.rows);
  if (width > std::numeric_limits<std::size_t>::max() / 3 / height)
    throw std::invalid_argument("capture image byte count overflow");
  const auto row = width * 3;
  Frame result{static_cast<std::uint32_t>(image.cols), static_cast<std::uint32_t>(image.rows), {}};
  result.data.reserve(row * height);
  for (int y = 0; y < image.rows; ++y) {
    const auto* pixels = image.ptr<std::uint8_t>(y);
    for (std::size_t x = 0; x < row; ++x) result.data.push_back(pixels[x]);
  }
  return result;
}
}
