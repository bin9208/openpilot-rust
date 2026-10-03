#include <chrono>
#include <cstring>
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <vector>
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

int run_client(const std::string &name) {
  std::vector<std::unique_ptr<VisionIpcClient>> clients;
  for (int stream = 0; stream < VISION_STREAM_MAX; ++stream) {
    auto client = std::make_unique<VisionIpcClient>(name, static_cast<VisionStreamType>(stream), false);
    if (!client->connect(false)) return 2;
    clients.push_back(std::move(client));
  }
  std::cout << "READY" << std::endl;
  std::string command;
  while (std::cin >> command) {
    if (command == "stop") {
      std::cout << "OK" << std::endl;
      return 0;
    }
    if (command != "receive") return 3;
    uint32_t frame;
    std::cin >> frame;
    for (auto &client : clients) {
      VisionIpcBufExtra extra{};
      VisionBuf *buffer = client->recv(&extra, 2000);
      if (!buffer || extra.frame_id != frame || extra.timestamp_sof != uint64_t{frame} * 1000 ||
          extra.timestamp_eof != uint64_t{frame} * 1000 + 100 || extra.valid != (frame % 2 == 0) ||
          buffer->width != 8 || buffer->height != 4 || buffer->stride != 16 || buffer->uv_offset != 64 ||
          buffer->len != 96 || buffer->mmap_len != 104 || buffer->get_frame_id() != frame) return 4;
      for (size_t i = 0; i < buffer->len; ++i) {
        if (static_cast<uint8_t *>(buffer->addr)[i] != static_cast<uint8_t>(i + frame)) return 5;
      }
    }
    std::cout << "OK" << std::endl;
  }
  return 6;
}

int main(int argc, char **argv) {
  if (argc == 3 && std::string(argv[1]) == "client") return run_client(argv[2]);
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
