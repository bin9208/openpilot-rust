#include <cstdint>
#include <cstring>
#include <iostream>
#include <memory>
#include "msgq/visionipc/visionipc_client.h"
#include "msgq/visionipc/visionipc_server.h"

int main() {
  auto server = std::make_unique<VisionIpcServer>("ioncontract");
  server->create_buffers_with_sizes(VISION_STREAM_ROAD, 1, 8, 4, 96, 16, 64);
  auto *buffer = server->get_buffer(VISION_STREAM_ROAD, 0);
  for (size_t i = 0; i < buffer->mmap_len; ++i) if (static_cast<uint8_t *>(buffer->addr)[i]) return 5;
  for (size_t i = 0; i < buffer->len; ++i) static_cast<uint8_t *>(buffer->addr)[i] = static_cast<uint8_t>(i + 42);
  server->start_listener();
  auto client = std::make_unique<VisionIpcClient>("ioncontract", VISION_STREAM_ROAD, false);
  if (!client->connect(true)) return 1;
  buffer->set_frame_id(42);
  VisionIpcBufExtra extra{42, 42000, 42100, true};
  server->send(buffer, &extra);
  auto *frame = client->recv(&extra, 2000);
  if (!frame || frame->get_frame_id() != 42 || extra.frame_id != 42) return 2;
  uint64_t checksum = 0;
  for (size_t i = 0; i < frame->len; ++i) {
    auto byte = static_cast<uint8_t *>(frame->addr)[i];
    if (byte != static_cast<uint8_t>(i + 42)) return 3;
    checksum += (i + 1) * byte;
  }
  client.reset();
  server.reset();
  VisionBuf raw{};
  raw.allocate(64);
  for (size_t i = 0; i < raw.mmap_len; ++i) if (static_cast<uint8_t *>(raw.addr)[i]) return 6;
  std::memset(raw.addr, 7, 64);
  uint64_t sum = 0;
  for (size_t i = 0; i < raw.len; ++i) sum += static_cast<uint8_t *>(raw.addr)[i];
  if (raw.free() != 0) std::perror("free raw VisionIPC buffer");
  if (sum != 448) return 4;
  std::cout << "{\"frame_id\":42,\"checksum\":" << checksum << ",\"raw_sum\":" << sum << "}" << std::endl;
}
