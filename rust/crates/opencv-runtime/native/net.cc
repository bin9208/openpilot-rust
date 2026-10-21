#include "bridge.h"
#include "check.h"
#include "dnn_context.h"
#include <algorithm>
namespace openpilot_opencv {
Net::~Net() noexcept {
  if (!gpu_context.empty()) {
    // UMat cleanup may use Queue::getDefault() for host synchronization. Release
    // graph allocations while our own context is bound, then restore the caller.
    try {
      DnnExecutionScope scope(&gpu_context);
      value = cv::dnn::Net();
      scope.finish();
    } catch (...) { /* destructors must not unwind across the Rust boundary */ }
  }
}
std::unique_ptr<Net> load_onnx_with_target(rust::Str path, std::uint8_t target) {
  const std::string filename(path);
  if (filename.find('\0') != std::string::npos) throw std::invalid_argument("ONNX path contains NUL");
  if (target > 1) throw std::invalid_argument("unsupported DNN target");
  std::string priority = "not_applicable_cpu";
  cv::ocl::OpenCLExecutionContext gpu;
  if (target == 1) gpu = make_gpu_context(priority);
  DnnExecutionScope scope(target == 1 ? &gpu : nullptr);
  if (target == 1) {
    if (!cv::ocl::useOpenCL() || cv::ocl::Device::getDefault().type() != cv::ocl::Device::TYPE_GPU)
      throw std::runtime_error("OpenCL FP32 requires an active real GPU context");
    const auto targets = cv::dnn::getAvailableTargets(cv::dnn::DNN_BACKEND_OPENCV);
    if (std::find(targets.begin(), targets.end(), cv::dnn::DNN_TARGET_OPENCL) == targets.end())
      throw std::runtime_error("OpenCV DNN provider does not advertise OpenCL FP32");
  }
  auto network = cv::dnn::readNetFromONNX(filename);
  network.setPreferableBackend(cv::dnn::DNN_BACKEND_OPENCV);
  network.setPreferableTarget(target == 1 ? cv::dnn::DNN_TARGET_OPENCL : cv::dnn::DNN_TARGET_CPU);
  auto net = std::make_unique<Net>(std::move(network));
  net->requested_target = target;
  net->priority_status = priority;
  if (target == 1) {
    net->gpu_context = gpu;
    net->device = gpu.getDevice().name();
    net->vendor = gpu.getDevice().vendorName();
    net->driver = gpu.getDevice().driverVersion();
  }
  scope.finish();
  return net;
}
void invalidate_net_report(Net& net) noexcept {
  net.graph_report_valid = false;
  net.opencl_active = false;
}
DnnReportData net_report(const Net& net, bool execution_valid) {
  DnnReportData report;
  report.requested_target = net.requested_target;
  report.device = net.device;
  report.vendor = net.vendor;
  report.driver = net.driver;
  report.priority_status = net.priority_status;
  report.opencl_active = execution_valid && net.opencl_active;
  report.graph_gpu_layers = execution_valid && net.graph_report_valid ? net.graph_gpu_layers : 0;
  report.graph_cpu_layers = execution_valid && net.graph_report_valid ? net.graph_cpu_layers : 0;
  return report;
}
namespace {
void cache_graph_report(Net& net) {
  // Same public field used by pinned Net::dump(), after allocation and global
  // fallback. Includes fused layers, not actual forward_ocl kernel successes.
  const auto names = net.value.getLayerNames();
  if (names.size() > UINT32_MAX) throw std::runtime_error("DNN layer count overflow");
  std::uint32_t gpu = 0, cpu = 0;
  for (const auto& name : names) {
    const auto layer = net.value.getLayer(net.value.getLayerId(name));
    if (layer.empty()) throw std::runtime_error("DNN graph layer unavailable");
    switch (layer->preferableTarget) {
      case cv::dnn::DNN_TARGET_OPENCL: ++gpu; break;
      case cv::dnn::DNN_TARGET_CPU: ++cpu; break;
      default: throw std::runtime_error("unexpected DNN graph precision/target");
    }
  }
  net.graph_gpu_layers = gpu;
  net.graph_cpu_layers = cpu;
  net.graph_report_valid = true;
}
}
rust::Vec<rust::String> output_names(const Net& net) {
  rust::Vec<rust::String> result;
  for (const auto& name : net.value.getUnconnectedOutLayersNames()) result.push_back(rust::String(name));
  return result;
}
rust::Vec<Tensor> forward(Net& net, rust::Slice<const float> input, rust::Slice<const std::int32_t> shape, rust::Slice<const rust::String> names) {
  const bool previous_graph_valid = net.graph_report_valid;
  invalidate_net_report(net);
  if (shape.empty() || shape.size() > 32) throw std::invalid_argument("DNN rank must be 1..32");
  std::size_t count = 1;
  for (const auto axis : shape) {
    if (axis <= 0 || count > std::numeric_limits<std::size_t>::max() / static_cast<std::size_t>(axis))
      throw std::invalid_argument("DNN shape product overflow or invalid axis");
    count *= static_cast<std::size_t>(axis);
  }
  if (count != input.size()) throw std::invalid_argument("DNN shape differs from input slice");
  std::vector<cv::String> requested;
  requested.reserve(names.size());
  for (const auto& name : names) {
    const std::string text(name);
    if (text.find('\0') != std::string::npos) throw std::invalid_argument("DNN output name contains NUL");
    requested.emplace_back(text);
  }
  const bool same_shape = net.report_shape.size() == shape.size() &&
    std::equal(shape.begin(), shape.end(), net.report_shape.begin());
  const bool refresh_graph = !previous_graph_valid || !same_shape || net.report_names != requested;
  DnnExecutionScope scope(net.requested_target == 1 ? &net.gpu_context : nullptr);
  if (net.requested_target == 1 &&
      (!cv::ocl::useOpenCL() || cv::ocl::Device::getDefault().ptr() != net.gpu_context.getDevice().ptr() ||
       cv::ocl::Device::getDefault().type() != cv::ocl::Device::TYPE_GPU))
    throw std::runtime_error("DNN GPU context lost or changed");
  // setInput may retain a Mat; clone first so no borrowed Rust pointer survives this call.
  const auto owned_input = cv::Mat(static_cast<int>(shape.size()), shape.data(), CV_32F,
    const_cast<float*>(input.data())).clone();
  net.value.setInput(owned_input);
  std::vector<cv::Mat> outputs;
  if (names.empty()) outputs.push_back(net.value.forward());
  else {
    net.value.forward(outputs, requested);
  }
  rust::Vec<Tensor> result;
  result.reserve(outputs.size());
  for (const auto& output : outputs) {
    if (output.type() != CV_32F || output.empty() || output.dims > 32)
      throw std::runtime_error("DNN returned unsupported output type or empty shape");
    Tensor tensor;
    for (int i = 0; i < output.dims; ++i) tensor.shape.push_back(output.size[i]);
    const auto contiguous = output.isContinuous() ? output : output.clone();
    const auto* values = contiguous.ptr<float>();
    tensor.values.reserve(contiguous.total());
    for (std::size_t i = 0; i < contiguous.total(); ++i) tensor.values.push_back(values[i]);
    result.push_back(std::move(tensor));
  }
  scope.finish();
  if (refresh_graph) {
    cache_graph_report(net);
    net.report_shape.assign(shape.begin(), shape.end());
    net.report_names = std::move(requested);
  } else net.graph_report_valid = true;
  if (net.requested_target == 1) net.opencl_active = cv::ocl::useOpenCL();
  return result;
}
}
