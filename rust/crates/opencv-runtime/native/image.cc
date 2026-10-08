#include "bridge.h"
#include "check.h"
#include <opencv2/imgproc.hpp>
#include <opencv2/core/version.hpp>
static_assert(CV_VERSION_MAJOR == 4 && CV_VERSION_MINOR == 13 && CV_VERSION_REVISION == 0, "pinned OpenCV4.13 required");
namespace openpilot_opencv {
void set_threads(std::int32_t count) {
  if (count <= 0) throw std::invalid_argument("thread count must be positive");
  cv::setNumThreads(count);
}
rust::Vec<std::uint8_t> resize(rust::Slice<const std::uint8_t> data, Layout layout, Dimensions output) {
  checked_size(output, layout.channels);
  const auto input = borrowed_image(data, layout);
  cv::Mat result;
  cv::resize(input, result, cv::Size(static_cast<int>(output.width), static_cast<int>(output.height)), 0, 0, cv::INTER_LINEAR);
  return owned_bytes(result);
}
rust::Vec<std::uint8_t> bgr_gray(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto input = borrowed_image(data, Layout{size, 3});
  cv::Mat result;
  cv::cvtColor(input, result, cv::COLOR_BGR2GRAY);
  return owned_bytes(result);
}
rust::Vec<std::uint8_t> nv12_rgb(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto pixels = checked_size(size, 1);
  if (size.width % 2 || size.height % 2 || pixels > std::numeric_limits<std::size_t>::max() - pixels / 2
      || data.size() != pixels + pixels / 2 || size.height > INT32_MAX / 3 * 2)
    throw std::invalid_argument("packed NV12 dimensions or slice mismatch");
  const cv::Mat input(static_cast<int>(size.height + size.height / 2), static_cast<int>(size.width), CV_8UC1,
    const_cast<std::uint8_t*>(data.data()));
  cv::Mat result;
  cv::cvtColor(input, result, cv::COLOR_YUV2RGB_NV12);
  return owned_bytes(result);
}
rust::Vec<std::uint8_t> mask_polygon(rust::Slice<const Point> points, Dimensions size) {
  checked_size(size, 1);
  if (points.size() < 3) throw std::invalid_argument("polygon mask needs three points");
  const auto polygon = checked_points(points);
  cv::Mat mask = cv::Mat::zeros(static_cast<int>(size.height), static_cast<int>(size.width), CV_8UC1);
  cv::fillPoly(mask, std::vector<std::vector<cv::Point>>{polygon}, cv::Scalar(255));
  return owned_bytes(mask);
}
Rect bounds(rust::Slice<const Point> points) {
  const auto result = cv::boundingRect(checked_points(points));
  if (result.width <= 0 || result.height <= 0) throw std::runtime_error("OpenCV returned invalid polygon bounds");
  return Rect{result.x, result.y, static_cast<std::uint32_t>(result.width), static_cast<std::uint32_t>(result.height)};
}
rust::Vec<std::uint8_t> mask_image(rust::Slice<const std::uint8_t> data, rust::Slice<const std::uint8_t> mask, Layout layout) {
  const auto image = borrowed_image(data, layout);
  const auto filter = borrowed_image(mask, Layout{layout.size, 1});
  cv::Mat result;
  cv::bitwise_and(image, image, result, filter);
  return owned_bytes(result);
}
}
