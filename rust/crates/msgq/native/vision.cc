#include "vision.h"
#include "openpilot-msgq/src/vision_bridge.rs.h"
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <stdexcept>

namespace openpilot_rust {
namespace {
void validate_name(const std::string &name) {
  const char *prefix = std::getenv("OPENPILOT_PREFIX");
  const auto component = [](const std::string &value) {
    return value.size() <= 40 && std::all_of(value.begin(), value.end(), [](unsigned char c) {
      return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
             (c >= '0' && c <= '9') || c == '_' || c == '-';
    });
  };
  if (name.empty() || !component(name) || (prefix && !component(prefix))) {
    throw std::invalid_argument("invalid VisionIPC server name or namespace");
  }
  if (std::getenv("CEREAL_FAKE")) throw std::invalid_argument("CEREAL_FAKE is unsupported");
}

void validate_buffer(const VisionBuf &buffer) {
  const size_t maximum = std::numeric_limits<size_t>::max();
  if (buffer.fd < 0 || !buffer.addr || !buffer.width || !buffer.height || buffer.width % 2 || buffer.height % 2 ||
      buffer.stride < buffer.width || buffer.height > maximum / buffer.stride ||
      buffer.uv_offset < buffer.stride * buffer.height || buffer.uv_offset > buffer.len ||
      buffer.stride * (buffer.height / 2) > buffer.len - buffer.uv_offset ||
      buffer.len > buffer.mmap_len || buffer.mmap_len - buffer.len < sizeof(uint64_t)) {
    throw std::runtime_error("invalid VisionIPC NV12 buffer layout");
  }
}
}

void validate_vision_name(const std::string &name) { validate_name(name); }

VisionConnection::VisionConnection(const std::string &name, VisionStreamType stream, bool conflate)
    : client_(name, stream, conflate) {}

bool VisionConnection::connect() {
  current_ = nullptr;
  imported_valid_ = false;
  if (!client_.connect(false)) return false;
  client_.connected = false;
  if (client_.num_buffers == 0) throw std::runtime_error("VisionIPC stream has no buffers");
  for (int i = 0; i < client_.num_buffers; ++i) validate_buffer(client_.buffers[i]);
  imported_valid_ = true;
  client_.connected = true;
  return true;
}

bool VisionConnection::connected() const { return client_.connected; }

ConnectionLayout VisionConnection::layout() const {
  if (client_.num_buffers <= 0) return {};
  // connect() validates every imported layout; only owned scalars cross this boundary.
  const VisionBuf &buffer = client_.buffers[0];
  return {buffer.width, buffer.height, buffer.stride, buffer.uv_offset, buffer.len, true};
}

VisionMetadata VisionConnection::receive(int32_t timeout_ms) {
  if (!client_.connected || timeout_ms < 0) throw std::invalid_argument("VisionIPC client is not connected or timeout is invalid");
  return receive_retained(timeout_ms);
}

VisionMetadata VisionConnection::receive_retained(int32_t timeout_ms) {
  if (!imported_valid_ || timeout_ms < 0) throw std::invalid_argument("VisionIPC has no validated imported buffers or timeout is invalid");
  VisionIpcBufExtra extra{};
  current_ = client_.recv(&extra, timeout_ms);
  if (!current_) return {};
  return {current_->width, current_->height, current_->stride, current_->uv_offset, current_->len,
          extra.frame_id, extra.timestamp_sof, extra.timestamp_eof, extra.valid, true,
          static_cast<size_t>(current_->idx), current_->fd};
}

BufferDescriptor VisionConnection::frame_descriptor() const {
  if (!current_ || current_->fd < 0) throw std::invalid_argument("VisionIPC descriptor requires a received frame");
  uint64_t frame_id;
  std::memcpy(&frame_id, static_cast<const uint8_t *>(current_->addr) + current_->len, sizeof(frame_id));
  return {current_->fd, current_->mmap_len, current_->len, current_->idx, current_->server_id, frame_id};
}

void VisionConnection::copy_frame(rust::Slice<uint8_t> destination) const {
  if (!current_ || destination.size() != current_->len) throw std::invalid_argument("VisionIPC copy requires an exact-sized frame destination");
  std::memcpy(destination.data(), current_->addr, current_->len);
}

std::unique_ptr<VisionConnection> open_vision(rust::Str name, int32_t stream, bool conflate) {
  const std::string server(name);
  validate_name(server);
  if (stream < VISION_STREAM_ROAD || stream >= VISION_STREAM_MAX) throw std::invalid_argument("invalid VisionIPC stream");
  return std::make_unique<VisionConnection>(server, static_cast<VisionStreamType>(stream), conflate);
}

uint32_t vision_streams(rust::Str name) {
  const std::string server(name);
  validate_name(server);
  uint32_t mask = 0;
  for (const auto stream : VisionIpcClient::getAvailableStreams(server, false)) {
    if (stream < VISION_STREAM_ROAD || stream >= VISION_STREAM_MAX) throw std::runtime_error("invalid advertised VisionIPC stream");
    mask |= uint32_t{1} << stream;
  }
  return mask;
}
}
