#include "dnn_context.h"
#include "dnn_context_device.h"
#include "opencl_priority.h"
#include <algorithm>
#include <cstdlib>
#include <cstdint>
#include <dlfcn.h>
#include <stdexcept>
#include <vector>

namespace openpilot_opencv {
namespace {
using namespace opencl_abi;
class OpenClApi {
public:
  OpenClApi() {
    // Match pinned OpenCV's Linux loader search, including an explicit override.
    const char* override = std::getenv("OPENCV_OPENCL_RUNTIME");
    const bool custom = override && *override;
    handle_ = dlopen(custom ? override : "libOpenCL.so", RTLD_NOW | RTLD_LOCAL);
    if (!handle_ && !custom) handle_ = dlopen("libOpenCL.so.1", RTLD_NOW | RTLD_LOCAL);
    if (!handle_) throw std::runtime_error("QCOM priority: cannot load stock OpenCL runtime");
    create = reinterpret_cast<Create>(dlsym(handle_, "clCreateContext"));
    info = reinterpret_cast<Info>(dlsym(handle_, "clGetContextInfo"));
    release = reinterpret_cast<Release>(dlsym(handle_, "clReleaseContext"));
    if (!create || !info || !release) {
      dlclose(handle_);
      throw std::runtime_error("QCOM priority: stock OpenCL context symbols unavailable");
    }
  }
  ~OpenClApi() { dlclose(handle_); }
  OpenClApi(const OpenClApi&) = delete;
  OpenClApi& operator=(const OpenClApi&) = delete;
  Create create;
  Info info;
  Release release;
private:
  void* handle_ = nullptr;
};

cv::ocl::Context low_priority_context(const cv::ocl::Device& device) {
  OpenClApi api;
  auto owner = make_low_priority_context(static_cast<Device>(device.ptr()), api.create, api.info, api.release);
  // Public fromHandle retains the context. owner releases only our reference,
  // including on exceptions; execution context/net keep their own references.
  auto context = cv::ocl::Context::fromHandle(owner.get());
  if (context.empty() || context.ndevices() != 1 || context.device(0).ptr() != device.ptr())
    throw std::runtime_error("QCOM priority: imported context device differs from selected GPU");
  return context;
}
}

cv::ocl::OpenCLExecutionContext make_gpu_context(std::string& priority_status) {
  if (!cv::ocl::haveOpenCL())
    throw std::runtime_error("OpenCL FP32 requested but provider/runtime has no OpenCL support");
  std::vector<cv::ocl::PlatformInfo> platforms;
  cv::ocl::getPlatfomsInfo(platforms);
  if (platforms.size() > 64) throw std::runtime_error("too many OpenCL platforms");
  cv::ocl::Device selected;
  for (const auto& platform : platforms) {
    const int count = platform.deviceNumber();
    if (count < 0 || count > 64) throw std::runtime_error("invalid OpenCL platform device count");
    for (int i = 0; i < count; ++i) {
      cv::ocl::Device candidate;
      platform.getDevice(candidate, i);
      // Match DNN's strict GPU check: a CPU ICD must never be accepted as GPU.
      if (candidate.type() != cv::ocl::Device::TYPE_GPU ||
          !candidate.available() || !candidate.compilerAvailable()) continue;
      if (selected.empty() || candidate.isExtensionSupported("cl_qcom_priority_hint")) selected = candidate;
      if (selected.isExtensionSupported("cl_qcom_priority_hint")) break;
    }
    if (!selected.empty() && selected.isExtensionSupported("cl_qcom_priority_hint")) break;
  }
  if (selected.empty()) throw std::runtime_error("OpenCL FP32 requires an available GPU with compiler; no GPU found");
  cv::ocl::Context context;
  if (selected.isExtensionSupported("cl_qcom_priority_hint")) {
    context = low_priority_context(selected);
    priority_status = "qcom_low_requested_verified";
  } else {
    context = cv::ocl::Context::fromDevice(selected);
    priority_status = "unsupported_qcom_priority_hint";
  }
  auto execution = create_context_execution(context, selected,
    [](const cv::ocl::Context& owned_context, const cv::ocl::Device& owned_device) {
      return cv::ocl::OpenCLExecutionContext::create(owned_context, owned_device);
    });
  if (execution.empty() || execution.getQueue().ptr() == nullptr ||
      execution.getDevice().ptr() != selected.ptr() || !execution.useOpenCL())
    throw std::runtime_error("OpenCL FP32 execution context/queue validation failed");
  return execution;
}

DnnExecutionScope::DnnExecutionScope(cv::ocl::OpenCLExecutionContext* gpu)
    : previous_(cv::ocl::OpenCLExecutionContext::getCurrentRef()), gpu_(gpu) {
  if (!previous_.empty()) previous_enabled_ = previous_.useOpenCL();
  if (gpu_) {
    gpu_->bind();
    cv::ocl::setUseOpenCL(true);
  } else {
    // In pinned ocl.cpp setUseOpenCL(false) touches TLS only when empty. Do not
    // call haveOpenCL/useOpenCL/getCurrent/Context::getDefault on the CPU path.
    cv::ocl::setUseOpenCL(false);
  }
}
void DnnExecutionScope::finish() {
  if (gpu_ && !finished_) {
    gpu_->getQueue().finish();
    finished_ = true;
  }
}
DnnExecutionScope::~DnnExecutionScope() noexcept {
  // Drain even on forward exceptions before the parent's GPU mutex is released.
  try { finish(); } catch (...) { /* the original forward exception is preserved */ }
  if (gpu_) {
    if (!previous_.empty()) {
      previous_.setUseOpenCL(previous_enabled_);
      previous_.bind();
    } else {
      // OpenCV's stock Scope does not restore an initially empty binding.
      cv::ocl::OpenCLExecutionContext::getCurrentRef().release();
      cv::ocl::setUseOpenCL(false);
    }
  } else if (!previous_.empty()) {
    // CPU requests never bind, even when the caller had an existing context.
    cv::ocl::setUseOpenCL(previous_enabled_);
  }
}
}
