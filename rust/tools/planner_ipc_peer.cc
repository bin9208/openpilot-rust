// 원래 msgq C++ source를 직접 링크한다. Python peer/relay나 Rust transport 대체는 없다.
#include <fstream>
#include <iomanip>
#include <iostream>
#include <iterator>
#include <map>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
#include "msgq/ipc.h"
#include "services.h"

static std::string decode(const std::string &hex) {
  if (hex.empty() || hex.size() % 2 != 0 || hex.size() > 8192) throw std::runtime_error("invalid path hex");
  std::string out;
  for (size_t i = 0; i < hex.size(); i += 2) {
    auto digit = [](char c) -> int {
      if (c >= '0' && c <= '9') return c - '0';
      if (c >= 'a' && c <= 'f') return c - 'a' + 10;
      throw std::runtime_error("invalid path hex digit");
    };
    const char c = static_cast<char>(digit(hex[i]) * 16 + digit(hex[i + 1]));
    if (c == '\0') throw std::runtime_error("NUL path");
    out.push_back(c);
  }
  return out;
}

static std::string encode(const std::string &path) {
  std::ostringstream out;
  out << std::hex << std::setfill('0');
  for (unsigned char c : path) out << std::setw(2) << unsigned(c);
  return out.str();
}

int main() {
  try {
    const std::vector<std::string> outputs = {"longitudinalPlan", "lateralPlan", "driverAssistance"};
    std::unique_ptr<Context> context(Context::create());
    std::map<std::string, std::unique_ptr<PubSocket>> publishers;
    std::map<std::string, std::unique_ptr<SubSocket>> subscribers;
    for (const auto *name : {"carControl", "carState", "controlsState", "liveParameters", "radarState",
                            "liveTracks", "modelV2", "selfdriveState", "carrotMan", "livePose"}) {
      publishers.emplace(name, PubSocket::create(context.get(), name, true, services.at(name).queue_size));
      if (!publishers.at(name)) throw std::runtime_error("input publisher unavailable");
    }
    size_t sequence = 0;
    std::cout << "READY" << std::endl;
    std::string line;
    while (std::getline(std::cin, line)) {
      if (line.size() > 16384) throw std::runtime_error("oversized peer command");
      std::istringstream input(line);
      std::string command;
      input >> command;
      if (command == "readers") {
        bool ready = true;
        for (auto &item : publishers) ready = item.second->all_readers_updated() && ready;
        std::cout << "READERS " << (ready ? 1 : 0) << std::endl;
      } else if (command == "subscribe") {
        if (!subscribers.empty()) throw std::runtime_error("duplicate subscribe");
        for (const auto &name : outputs) {
          subscribers.emplace(name, SubSocket::create(context.get(), name, "127.0.0.1", false, true, services.at(name).queue_size));
          if (!subscribers.at(name)) throw std::runtime_error("output subscriber unavailable");
          subscribers.at(name)->setTimeout(0);
        }
        std::cout << "OK" << std::endl;
      } else if (command == "publish") {
        std::string name, path;
        if (!(input >> name >> path)) throw std::runtime_error("publish requires topic and path");
        std::ifstream file(decode(path), std::ios::binary);
        if (!file) throw std::runtime_error("cannot read raw input packet");
        std::vector<char> bytes{std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
        if (bytes.empty() || bytes.size() > 16777216) throw std::runtime_error("raw packet outside admitted bound");
        if (publishers.at(name)->send(bytes.data(), bytes.size()) < 0) throw std::runtime_error("publish failed");
        std::cout << "OK" << std::endl;
      } else if (command == "drain") {
        std::string path;
        if (!(input >> path) || subscribers.size() != outputs.size()) throw std::runtime_error("drain requires subscribers and output");
        const auto directory = decode(path);
        size_t batch = 0;
        for (const auto &name : outputs) {
          while (std::unique_ptr<Message> message{subscribers.at(name)->receive(true)}) {
            if (++batch > 4096) throw std::runtime_error("publication batch exceeds bound");
            std::ostringstream destination;
            destination << directory << '/' << std::setfill('0') << std::setw(4) << sequence++ << '-' << name << ".bin";
            std::ofstream file(destination.str(), std::ios::binary);
            file.write(message->getData(), message->getSize());
            if (!file) throw std::runtime_error("cannot save output packet");
            std::cout << "PACKET " << name << ' ' << encode(destination.str()) << '\n';
          }
        }
        std::cout << "END" << std::endl;
      } else if (command == "stop") {
        std::cout << "OK" << std::endl;
        return 0;
      } else {
        throw std::runtime_error("unknown planner peer command");
      }
      std::string extra;
      if (input >> extra) throw std::runtime_error("extra planner peer arguments");
    }
    return 0;
  } catch (const std::exception &error) {
    std::cerr << error.what() << std::endl;
    return 1;
  }
}
