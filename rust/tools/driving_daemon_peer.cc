#include <algorithm>
#include <chrono>
#include <fstream>
#include <iostream>
#include <iterator>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
#include "msgq/ipc.h"
#include "msgq/visionipc/visionipc_client.h"
#include "msgq/visionipc/visionipc_server.h"
#include "services.h"

std::vector<char> read_file(const std::string &path) {
  std::ifstream file(path, std::ios::binary);
  if (!file) throw std::runtime_error("cannot read input file");
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

int main(int argc, char **argv) {
  try {
    if (argc != 7) throw std::runtime_error("width height stride uv-offset bytes dual|road|wide required");
    const size_t width = std::stoul(argv[1]), height = std::stoul(argv[2]);
    const size_t stride = std::stoul(argv[3]), uv_offset = std::stoul(argv[4]), bytes = std::stoul(argv[5]);
    const std::string mode = argv[6];
    if (mode != "dual" && mode != "road" && mode != "wide") throw std::runtime_error("invalid stream selection");
    std::unique_ptr<Context> context(Context::create());
    std::map<std::string, std::unique_ptr<PubSocket>> publishers;
    std::map<std::string, std::unique_ptr<SubSocket>> subscribers;
    for (const auto *name : {"deviceState", "carState", "roadCameraState", "liveCalibration", "driverMonitoringState",
                            "carControl", "liveDelay", "carrotMan", "radarState"}) {
      publishers.emplace(name, PubSocket::create(context.get(), name, true, services.at(name).queue_size));
      if (!publishers.at(name)) throw std::runtime_error("input publisher unavailable");
    }
    for (const auto *name : {"modelV2", "drivingModelData", "cameraOdometry"}) {
      subscribers.emplace(name, SubSocket::create(context.get(), name, "127.0.0.1", false, true, services.at(name).queue_size));
      if (!subscribers.at(name)) throw std::runtime_error("output subscriber unavailable");
    }
    VisionIpcServer server("camerad");
    std::vector<VisionStreamType> streams;
    if (mode != "wide") streams.push_back(VISION_STREAM_ROAD);
    if (mode != "road") streams.push_back(VISION_STREAM_WIDE_ROAD);
    for (auto stream : streams) server.create_buffers_with_sizes(stream, 4, width, height, bytes, stride, uv_offset);
    server.start_listener();
    bool ready = false;
    for (int attempt = 0; attempt < 200; ++attempt) {
      if (VisionIpcClient::getAvailableStreams("camerad", false).size() == streams.size()) { ready = true; break; }
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    if (!ready) throw std::runtime_error("camera listener unavailable");
    std::cout << "READY" << std::endl;
    std::string command;
    while (std::cin >> command) {
      if (command == "publish") {
        std::string topic, path;
        std::cin >> topic >> path;
        const auto data = read_file(path);
        if (publishers.at(topic)->send(const_cast<char *>(data.data()), data.size()) < 0) throw std::runtime_error("input send failed");
      } else if (command == "send") {
        uint32_t frame;
        std::string main_path, extra_path;
        std::cin >> frame >> main_path >> extra_path;
        const auto main_data = read_file(main_path);
        const auto extra_data = mode == "dual" ? read_file(extra_path) : std::vector<char>{};
        for (auto stream : streams) {
          const bool wide = stream == VISION_STREAM_WIDE_ROAD;
          const auto &data = wide && mode == "dual" ? extra_data : main_data;
          if (data.size() != bytes) throw std::runtime_error("wrong frame size");
          auto *buffer = server.get_buffer(stream);
          std::copy(data.begin(), data.end(), static_cast<char *>(buffer->addr));
          buffer->set_frame_id(frame);
          const uint64_t sof = uint64_t{frame} * 50000000 + (wide && mode == "dual" ? 1000000 : 0);
          VisionIpcBufExtra metadata{frame, sof, sof + 1000, frame % 2 == 0};
          server.send(buffer, &metadata);
        }
      } else if (command == "receive") {
        int timeout;
        std::string topic, path;
        std::cin >> topic >> timeout >> path;
        auto &socket = subscribers.at(topic);
        socket->setTimeout(timeout);
        std::unique_ptr<Message> message(socket->receive());
        if (!message) { std::cout << "TIMEOUT" << std::endl; continue; }
        std::ofstream output(path, std::ios::binary);
        output.write(static_cast<const char *>(message->getData()), message->getSize());
        if (!output) throw std::runtime_error("cannot save output message");
      } else if (command == "stop") {
        std::cout << "OK" << std::endl;
        return 0;
      } else {
        throw std::runtime_error("unknown command");
      }
      std::cout << "OK" << std::endl;
    }
    return 0;
  } catch (const std::exception &error) {
    std::cerr << error.what() << std::endl;
    return 1;
  }
}
