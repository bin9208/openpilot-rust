#pragma once
#include <memory>
#include <vector>
#include <opencv2/dnn.hpp>
#include <opencv2/core/ocl.hpp>
#include "rust/cxx.h"
namespace openpilot_opencv {
struct DnnReportData;
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
  ~Net() noexcept;
  // Declaration order keeps context alive until every net allocation is destroyed.
  cv::ocl::OpenCLExecutionContext gpu_context;
  cv::dnn::Net value;
  std::uint8_t requested_target = 0;
  std::string device, vendor, driver, priority_status = "not_applicable_cpu";
  bool opencl_active = false;
  bool graph_report_valid = false;
  std::uint32_t graph_gpu_layers = 0, graph_cpu_layers = 0;
  std::vector<std::int32_t> report_shape;
  std::vector<std::string> report_names;
};
void set_threads(std::int32_t count);
rust::Vec<std::uint8_t> resize(rust::Slice<const std::uint8_t>, Layout, Dimensions);
rust::Vec<std::uint8_t> resize_gray_roi(rust::Slice<const std::uint8_t>, Dimensions, std::size_t, Rect, Dimensions);
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
rust::Vec<std::uint8_t> nv12_masked_resize_rgb(rust::Slice<const std::uint8_t>, Dimensions, rust::Slice<const std::uint8_t>, Dimensions);
rust::Vec<std::uint8_t> nv12_planes_rgb(rust::Slice<const std::uint8_t>, Dimensions, std::size_t, std::size_t);
rust::Vec<std::uint8_t> mask_polygon(rust::Slice<const Point>, Dimensions);
Rect bounds(rust::Slice<const Point>);
rust::Vec<std::uint8_t> mask_image(rust::Slice<const std::uint8_t>, rust::Slice<const std::uint8_t>, Layout);
std::unique_ptr<Net> load_onnx_with_target(rust::Str path, std::uint8_t target);
DnnReportData net_report(const Net&, bool execution_valid);
void invalidate_net_report(Net&) noexcept;
rust::Vec<rust::String> output_names(const Net&);
rust::Vec<Tensor> forward(Net&, rust::Slice<const float>, rust::Slice<const std::int32_t>, rust::Slice<const rust::String>);
}
