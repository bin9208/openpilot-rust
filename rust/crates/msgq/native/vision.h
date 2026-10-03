#pragma once
#include <memory>
#include "rust/cxx.h"
#include "msgq/visionipc/visionipc_client.h"

namespace openpilot_rust {
struct VisionMetadata;
struct ConnectionLayout;
struct BufferDescriptor;
class VisionConnection final {
public:
  VisionConnection(const std::string &name, VisionStreamType stream, bool conflate);
  bool connect();
  bool connected() const;
  ConnectionLayout layout() const;
  VisionMetadata receive(int32_t timeout_ms);
  VisionMetadata receive_retained(int32_t timeout_ms);
  BufferDescriptor frame_descriptor() const;
  void copy_frame(rust::Slice<uint8_t> destination) const;
private:
  VisionIpcClient client_;
  VisionBuf *current_ = nullptr;
  bool imported_valid_ = false;
};
std::unique_ptr<VisionConnection> open_vision(rust::Str name, int32_t stream, bool conflate);
uint32_t vision_streams(rust::Str name);
void validate_vision_name(const std::string &name);
}
