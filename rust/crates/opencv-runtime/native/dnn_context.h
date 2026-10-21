#pragma once
#include <opencv2/core/ocl.hpp>
#include <string>

namespace openpilot_opencv {
// Only GPU requests call this. No process-global OpenCV context is attached.
cv::ocl::OpenCLExecutionContext make_gpu_context(std::string& priority_status);

// CPU: disable OpenCL using noninitializing TLS APIs, without binding a context.
// GPU: bind the net's context and restore the prior binding, including empty TLS.
class DnnExecutionScope {
public:
  explicit DnnExecutionScope(cv::ocl::OpenCLExecutionContext* gpu);
  ~DnnExecutionScope() noexcept;
  DnnExecutionScope(const DnnExecutionScope&) = delete;
  DnnExecutionScope& operator=(const DnnExecutionScope&) = delete;
  void finish();
private:
  cv::ocl::OpenCLExecutionContext previous_;
  cv::ocl::OpenCLExecutionContext* gpu_;
  bool previous_enabled_ = false;
  bool finished_ = false;
};
}
