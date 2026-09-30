#include <chrono>
#include <fstream>
#include <iostream>
#include <iterator>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
#include "msgq/ipc.h"
#include "msgq/visionipc/visionipc_client.h"
#include "msgq/visionipc/visionipc_server.h"

std::vector<char> read_file(const std::string &path) {
  std::ifstream file(path, std::ios::binary);
  if (!file) throw std::runtime_error("cannot read input file");
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

int main(int argc, char **argv) {
  try {
    if (argc != 6) throw std::runtime_error("width height stride uv-offset bytes required");
    const size_t width = std::stoul(argv[1]), height = std::stoul(argv[2]);
    const size_t stride = std::stoul(argv[3]), uv_offset = std::stoul(argv[4]), bytes = std::stoul(argv[5]);
    std::unique_ptr<Context> context(Context::create());
    std::unique_ptr<PubSocket> calibration(PubSocket::create(context.get(), "liveCalibration"));
    std::unique_ptr<SubSocket> driver(SubSocket::create(context.get(), "driverStateV2"));
    if (!calibration || !driver) throw std::runtime_error("failed native message sockets");
    VisionIpcServer server("camerad");
    server.create_buffers_with_sizes(VISION_STREAM_DRIVER, 4, width, height, bytes, stride, uv_offset);
    server.start_listener();
    bool ready = false;
    for (int attempt = 0; attempt < 200; ++attempt) {
      if (!VisionIpcClient::getAvailableStreams("camerad", false).empty()) { ready = true; break; }
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    if (!ready) throw std::runtime_error("camera listener unavailable");
    std::cout << "READY" << std::endl;
    std::string command;
    while (std::cin >> command) {
      if (command == "calib") {
        std::string path;
        std::cin >> path;
        const auto data = read_file(path);
        if (calibration->send(const_cast<char *>(data.data()), data.size()) < 0) throw std::runtime_error("calibration send failed");
      } else if (command == "send") {
        uint32_t frame;
        std::string path;
        std::cin >> frame >> path;
        const auto data = read_file(path);
        if (data.size() != bytes) throw std::runtime_error("wrong frame size");
        auto *buffer = server.get_buffer(VISION_STREAM_DRIVER);
        std::copy(data.begin(), data.end(), static_cast<char *>(buffer->addr));
        buffer->set_frame_id(frame);
        VisionIpcBufExtra extra{frame, uint64_t{frame} * 50000000, uint64_t{frame} * 50000000 + 1000, frame % 2 == 0};
        server.send(buffer, &extra);
      } else if (command == "receive") {
        int timeout;
        std::string path;
        std::cin >> timeout >> path;
        driver->setTimeout(timeout);
        std::unique_ptr<Message> message(driver->receive());
        if (!message) { std::cout << "TIMEOUT" << std::endl; continue; }
        std::ofstream output(path, std::ios::binary);
        output.write(static_cast<const char *>(message->getData()), message->getSize());
        if (!output) throw std::runtime_error("cannot save driver message");
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
