#include <chrono>
#include <cstring>
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include "msgq/visionipc/visionipc_server.h"
#include "msgq/visionipc/visionipc_client.h"

std::unique_ptr<VisionIpcServer> start_server(bool malformed = false, size_t length = 96) {
  auto server = std::make_unique<VisionIpcServer>("rustvision");
  for (int stream = 0; stream < VISION_STREAM_MAX; ++stream) {
    server->create_buffers_with_sizes(static_cast<VisionStreamType>(stream), 4, malformed ? 7 : 8, 4, length, 16, 64);
  }
  server->start_listener();
  for (int attempt = 0; attempt < 200; ++attempt) {
    if (VisionIpcClient::getAvailableStreams("rustvision", false).size() == VISION_STREAM_MAX) return server;
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  throw std::runtime_error("native test server did not start");
}

int main() {
  auto server = start_server();
  std::cout << "READY" << std::endl;
  std::string command;
  while (std::cin >> command) {
    if (command == "send") {
      uint32_t frame;
      std::cin >> frame;
      for (int stream = 0; stream < VISION_STREAM_MAX; ++stream) {
        auto *buffer = server->get_buffer(static_cast<VisionStreamType>(stream));
        for (size_t i = 0; i < buffer->len; ++i) static_cast<uint8_t *>(buffer->addr)[i] = static_cast<uint8_t>(i + frame);
        uint64_t stored_frame = frame;
        std::memcpy(static_cast<uint8_t *>(buffer->addr) + buffer->len, &stored_frame, sizeof(stored_frame));
        VisionIpcBufExtra extra{frame, uint64_t{frame} * 1000, uint64_t{frame} * 1000 + 100, frame % 2 == 0};
        server->send(buffer, &extra);
      }
    } else if (command == "restart") {
      server.reset();
      server = start_server();
    } else if (command == "invalid-layout") {
      server.reset();
      server = start_server(true);
    } else if (command == "unaligned-buffer") {
      server.reset();
      server = start_server(false, 98);
    } else if (command == "stop") {
      server.reset();
      std::cout << "OK" << std::endl;
      return 0;
    } else {
      return 1;
    }
    std::cout << "OK" << std::endl;
  }
}
