#pragma once
#include "openpilot-opencv-runtime/src/bridge.rs.h"
#include <limits>
#include <stdexcept>
#include <vector>
namespace openpilot_opencv {
inline std::size_t checked_size(Dimensions size, std::uint8_t channels) {
  if (!size.width || !size.height || size.width > INT32_MAX || size.height > INT32_MAX || (channels != 1 && channels != 3))
    throw std::invalid_argument("invalid packed image dimensions or channels");
  const auto pixels = std::uint64_t{size.width} * size.height;
  if (pixels > std::numeric_limits<std::size_t>::max() / channels)
    throw std::invalid_argument("packed image size overflow");
  return static_cast<std::size_t>(pixels) * channels;
}
inline cv::Mat borrowed_image(rust::Slice<const std::uint8_t> data, Layout layout) {
  if (data.size() != checked_size(layout.size, layout.channels)) throw std::invalid_argument("packed image slice length mismatch");
  // OpenCV's InputArray consumes this header synchronously and never writes input pixels.
  return cv::Mat(static_cast<int>(layout.size.height), static_cast<int>(layout.size.width),
    CV_MAKETYPE(CV_8U, layout.channels), const_cast<std::uint8_t*>(data.data()));
}
inline rust::Vec<std::uint8_t> owned_bytes(const cv::Mat& value) {
  if (value.depth() != CV_8U) throw std::runtime_error("OpenCV returned a non-byte image");
  const cv::Mat contiguous = value.isContinuous() ? value : value.clone();
  const auto count = contiguous.total() * contiguous.elemSize();
  rust::Vec<std::uint8_t> output;
  output.reserve(count);
  for (std::size_t i = 0; i < count; ++i) output.push_back(contiguous.data[i]);
  return output;
}
inline std::vector<cv::Point> checked_points(rust::Slice<const Point> points) {
  if (points.empty() || points.size() > INT32_MAX) throw std::invalid_argument("invalid polygon point count");
  std::vector<cv::Point> result;
  result.reserve(points.size());
  int left = points[0].x, right = left, top = points[0].y, bottom = top;
  for (const auto& point : points) {
    left = std::min(left, point.x); right = std::max(right, point.x);
    top = std::min(top, point.y); bottom = std::max(bottom, point.y);
    result.emplace_back(point.x, point.y);
  }
  if (std::int64_t{right} - left + 1 > INT32_MAX || std::int64_t{bottom} - top + 1 > INT32_MAX)
    throw std::invalid_argument("polygon rectangle span overflow");
  return result;
}
}
