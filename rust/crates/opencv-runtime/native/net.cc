#include "bridge.h"
#include "check.h"
namespace openpilot_opencv {
std::unique_ptr<Net> load_onnx(rust::Str path) {
  const std::string filename(path);
  if (filename.find('\0') != std::string::npos) throw std::invalid_argument("ONNX path contains NUL");
  auto network = cv::dnn::readNetFromONNX(filename);
  network.setPreferableBackend(cv::dnn::DNN_BACKEND_OPENCV);
  network.setPreferableTarget(cv::dnn::DNN_TARGET_CPU);
  return std::make_unique<Net>(std::move(network));
}
rust::Vec<rust::String> output_names(const Net& net) {
  rust::Vec<rust::String> result;
  for (const auto& name : net.value.getUnconnectedOutLayersNames()) result.push_back(rust::String(name));
  return result;
}
rust::Vec<Tensor> forward(Net& net, rust::Slice<const float> input, rust::Slice<const std::int32_t> shape, rust::Slice<const rust::String> names) {
  if (shape.empty() || shape.size() > 32) throw std::invalid_argument("DNN rank must be 1..32");
  std::size_t count = 1;
  for (const auto axis : shape) {
    if (axis <= 0 || count > std::numeric_limits<std::size_t>::max() / static_cast<std::size_t>(axis))
      throw std::invalid_argument("DNN shape product overflow or invalid axis");
    count *= static_cast<std::size_t>(axis);
  }
  if (count != input.size()) throw std::invalid_argument("DNN shape differs from input slice");
  // setInput may retain a Mat; clone first so no borrowed Rust pointer survives this call.
  const auto owned_input = cv::Mat(static_cast<int>(shape.size()), shape.data(), CV_32F,
    const_cast<float*>(input.data())).clone();
  net.value.setInput(owned_input);
  std::vector<cv::Mat> outputs;
  if (names.empty()) outputs.push_back(net.value.forward());
  else {
    std::vector<cv::String> requested;
    requested.reserve(names.size());
    for (const auto& name : names) {
      const std::string text(name);
      if (text.find('\0') != std::string::npos) throw std::invalid_argument("DNN output name contains NUL");
      requested.emplace_back(text);
    }
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
  return result;
}
}
