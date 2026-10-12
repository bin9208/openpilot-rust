#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <vector>
#include "msgq/visionipc/visionipc_server.h"
#include "msgq/visionipc/visionipc_client.h"

int main() {
  std::unique_ptr<VisionIpcServer> server;
  std::vector<VisionStreamType> streams;
  std::string command;
  std::cout << "READY" << std::endl;
  while (std::cin >> command) {
    if (command == "streams") {
      std::string mode;
      std::cin >> mode;
      server.reset();
      streams.clear();
      if (mode == "road" || mode == "both") streams.push_back(VISION_STREAM_ROAD);
      if (mode == "wide" || mode == "both") streams.push_back(VISION_STREAM_WIDE_ROAD);
      if (!streams.empty()) {
        server = std::make_unique<VisionIpcServer>("rustcluster");
        for (auto stream : streams) {
          const bool wide = stream == VISION_STREAM_WIDE_ROAD;
          server->create_buffers_with_sizes(stream, 4, wide ? 12 : 8, wide ? 6 : 4,
                                           wide ? 168 : 96, 16, wide ? 120 : 64);
        }
        server->start_listener();
        bool ready = false;
        for (int attempt = 0; attempt < 200; ++attempt) {
          if (VisionIpcClient::getAvailableStreams("rustcluster", false).size() == streams.size()) {
            ready = true;
            break;
          }
          std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (!ready) return 4;
      }
    } else if (command == "send") {
      std::string target;
      uint32_t frame;
      std::cin >> target >> frame;
      const auto stream = target == "road" ? VISION_STREAM_ROAD : VISION_STREAM_WIDE_ROAD;
      if (!server) return 2;
      auto *buffer = server->get_buffer(stream);
      if (!buffer) return 3;
      for (size_t i = 0; i < buffer->len; ++i)
        static_cast<uint8_t *>(buffer->addr)[i] = static_cast<uint8_t>(i + frame);
      buffer->set_frame_id(frame);
      VisionIpcBufExtra extra{frame, uint64_t{frame} * 1000, uint64_t{frame} * 1000 + 100,
                             frame % 2 == 0};
      server->send(buffer, &extra);
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
#include <chrono>
