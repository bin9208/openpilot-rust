#pragma once
#include <memory>
#include <opencv2/dnn.hpp>
#include "rust/cxx.h"
namespace openpilot_opencv {
struct Dimensions;
struct Layout;
struct Point;
struct Rect;
struct Tensor;
struct Component;
struct Components;
struct Contour;
class Net {
public:
  explicit Net(cv::dnn::Net value) : value(std::move(value)) {}
  cv::dnn::Net value;
};
void set_threads(std::int32_t count);
rust::Vec<std::uint8_t> resize(rust::Slice<const std::uint8_t>, Layout, Dimensions);
rust::Vec<std::uint8_t> bgr_gray(rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<std::uint8_t> rgb_gray(rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<std::uint8_t> rgb_hsv(rust::Slice<const std::uint8_t>, Dimensions);
Components components(rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<std::uint8_t> morphology_open(rust::Slice<const std::uint8_t>, Dimensions, Dimensions);
rust::Vec<std::uint8_t> morphology_tophat(rust::Slice<const std::uint8_t>, Dimensions, Dimensions);
rust::Vec<float> distance_transform(rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<Contour> external_contours(rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<float> match_template(rust::Slice<const std::uint8_t>, Dimensions, rust::Slice<const std::uint8_t>, Dimensions, rust::Slice<const std::uint8_t>);
rust::Vec<float> match_template_ccorr(rust::Slice<const std::uint8_t>, Dimensions, rust::Slice<const std::uint8_t>, Dimensions, rust::Slice<const std::uint8_t>);
rust::Vec<std::uint8_t> nv12_rgb(rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<std::uint8_t> nv12_planes_rgb(rust::Slice<const std::uint8_t>, Dimensions, std::size_t, std::size_t);
rust::Vec<std::uint8_t> mask_polygon(rust::Slice<const Point>, Dimensions);
Rect bounds(rust::Slice<const Point>);
rust::Vec<std::uint8_t> mask_image(rust::Slice<const std::uint8_t>, rust::Slice<const std::uint8_t>, Layout);
std::unique_ptr<Net> load_onnx(rust::Str path);
rust::Vec<rust::String> output_names(const Net&);
rust::Vec<Tensor> forward(Net&, rust::Slice<const float>, rust::Slice<const std::int32_t>, rust::Slice<const rust::String>);
}
