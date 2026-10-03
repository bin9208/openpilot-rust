#include "vision_server.h"
#include "vision.h"
#include "openpilot-msgq/src/vision_bridge.rs.h"
#include <cstring>
#include <cstdio>
#include <limits>
#include <stdexcept>

namespace openpilot_rust {
namespace {
void validate_length(size_t length) {
  if (length == 0 || length % alignof(uint64_t) != 0 || length > std::numeric_limits<size_t>::max() - sizeof(uint64_t)) {
    throw std::invalid_argument("invalid VisionIPC allocation length");
  }
}
void write(VisionBuf &buffer, size_t offset, rust::Slice<const uint8_t> bytes) {
  if (offset > buffer.len || bytes.size() > buffer.len - offset) {
    throw std::invalid_argument("VisionIPC write exceeds buffer");
  }
  if (bytes.size()) std::memcpy(static_cast<uint8_t*>(buffer.addr) + offset, bytes.data(), bytes.size());
}
void copy(const VisionBuf &buffer, rust::Slice<uint8_t> bytes) {
  if (bytes.size() != buffer.len) throw std::invalid_argument("VisionIPC copy requires exact buffer length");
  std::memcpy(bytes.data(), buffer.addr, buffer.len);
}
}

VisionPublisher::VisionPublisher(const std::string &name) : server_(name) {}

void VisionPublisher::create_stream(int32_t stream, size_t count, const ConnectionLayout &layout) {
  if (started_ || stream < 0 || stream >= VISION_STREAM_MAX || counts_[stream] != 0 || count == 0 || count >= VISIONIPC_MAX_FDS) {
    throw std::invalid_argument("invalid VisionIPC stream creation");
  }
  validate_length(layout.len);
  if (!layout.width || !layout.height || layout.width % 2 || layout.height % 2 || layout.stride < layout.width ||
      layout.height > std::numeric_limits<size_t>::max() / layout.stride || layout.uv_offset < layout.stride * layout.height ||
      layout.uv_offset > layout.len || layout.stride * (layout.height / 2) > layout.len - layout.uv_offset) {
    throw std::invalid_argument("invalid VisionIPC stream layout");
  }
  server_.create_buffers_with_sizes(static_cast<VisionStreamType>(stream), count, layout.width, layout.height,
                                    layout.len, layout.stride, layout.uv_offset);
  counts_[stream] = count;
}

void VisionPublisher::start_listener() {
  if (started_) throw std::invalid_argument("VisionIPC listener already started");
  server_.start_listener();
  started_ = true;
}

VisionBuf *VisionPublisher::buffer(int32_t stream, size_t index) {
  if (stream < 0 || stream >= VISION_STREAM_MAX || index >= counts_[stream]) throw std::out_of_range("VisionIPC buffer index");
  return server_.get_buffer(static_cast<VisionStreamType>(stream), static_cast<int>(index));
}

int32_t VisionPublisher::descriptor(int32_t stream, size_t index) { return buffer(stream, index)->fd; }
void VisionPublisher::write_buffer(int32_t stream, size_t index, size_t offset, rust::Slice<const uint8_t> bytes) {
  write(*buffer(stream, index), offset, bytes);
}
void VisionPublisher::copy_buffer(int32_t stream, size_t index, rust::Slice<uint8_t> bytes) {
  copy(*buffer(stream, index), bytes);
}
void VisionPublisher::publish(int32_t stream, size_t index, const VisionMetadata &metadata) {
  auto *image = buffer(stream, index);
  image->set_frame_id(metadata.frame_id);
  VisionIpcBufExtra extra{metadata.frame_id, metadata.timestamp_sof, metadata.timestamp_eof, metadata.valid};
  server_.send(image, &extra);
}

RawVisionBuffer::RawVisionBuffer(size_t length) { validate_length(length); buffer_.allocate(length); }
RawVisionBuffer::~RawVisionBuffer() { if (buffer_.free() != 0) std::perror("free raw VisionIPC buffer"); }
int32_t RawVisionBuffer::descriptor() const { return buffer_.fd; }
void RawVisionBuffer::write_buffer(size_t offset, rust::Slice<const uint8_t> bytes) { write(buffer_, offset, bytes); }
void RawVisionBuffer::copy_buffer(rust::Slice<uint8_t> bytes) const { copy(buffer_, bytes); }

std::unique_ptr<VisionPublisher> open_vision_server(rust::Str name) {
  const std::string server(name);
  validate_vision_name(server);
  return std::make_unique<VisionPublisher>(server);
}
std::unique_ptr<RawVisionBuffer> allocate_raw_vision(size_t length) { return std::make_unique<RawVisionBuffer>(length); }
}
