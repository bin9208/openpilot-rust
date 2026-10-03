#pragma once
#include <array>
#include <memory>
#include "rust/cxx.h"
#include "msgq/visionipc/visionipc_server.h"

namespace openpilot_rust {
struct ConnectionLayout;
struct VisionMetadata;

class VisionPublisher final {
public:
  explicit VisionPublisher(const std::string &name);
  void create_stream(int32_t stream, size_t count, const ConnectionLayout &layout);
  void start_listener();
  int32_t descriptor(int32_t stream, size_t index);
  void write_buffer(int32_t stream, size_t index, size_t offset, rust::Slice<const uint8_t> bytes);
  void copy_buffer(int32_t stream, size_t index, rust::Slice<uint8_t> bytes);
  void publish(int32_t stream, size_t index, const VisionMetadata &metadata);
private:
  VisionBuf *buffer(int32_t stream, size_t index);
  VisionIpcServer server_;
  std::array<size_t, VISION_STREAM_MAX> counts_{};
  bool started_ = false;
};

class RawVisionBuffer final {
public:
  explicit RawVisionBuffer(size_t length);
  ~RawVisionBuffer();
  int32_t descriptor() const;
  void write_buffer(size_t offset, rust::Slice<const uint8_t> bytes);
  void copy_buffer(rust::Slice<uint8_t> bytes) const;
private:
  VisionBuf buffer_;
};

std::unique_ptr<VisionPublisher> open_vision_server(rust::Str name);
std::unique_ptr<RawVisionBuffer> allocate_raw_vision(size_t length);
}
