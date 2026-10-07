#include "msgq/ipc.h"
#include <iostream>
#include <memory>
#include <chrono>
#include <string>
#include <thread>
#include <vector>
#include <cstring>

std::vector<uint8_t> payload(uint32_t value, size_t length) {
  std::vector<uint8_t> bytes(length);
  for (size_t i = 0; i < length; ++i) bytes[i] = static_cast<uint8_t>(i + value);
  std::memcpy(bytes.data(), &value, sizeof(value));
  return bytes;
}

int scripted(bool conflate) {
  std::unique_ptr<Context> context(Context::create());
  std::unique_ptr<SubSocket> subscriber(SubSocket::create(context.get(), "rustToNative", "127.0.0.1", conflate, false, 4096));
  std::unique_ptr<PubSocket> publisher(PubSocket::create(context.get(), "nativeToRust", false, 4096));
  if (!subscriber || !publisher) return 11;
  subscriber->setTimeout(2000);
  std::cout << "READY" << std::endl;
  std::string command;
  while (std::cin >> command) {
    if (command == "stop") { std::cout << "OK" << std::endl; return 0; }
    if (command == "send" || command == "receive" || command == "burst") {
      uint32_t value;
      size_t length;
      std::cin >> value >> length;
      if (length < 4 || length > 1024) return 12;
      if (command == "receive") {
        std::cout << "WAIT" << std::endl;
        std::unique_ptr<Message> message(subscriber->receive());
        auto expected = payload(value, length);
        if (!message || message->getSize() != length || std::memcmp(message->getData(), expected.data(), length)) return 13;
      } else {
        const uint32_t count = command == "burst" ? value : 1;
        for (uint32_t i = 0; i < count; ++i) {
          auto bytes = payload(command == "burst" ? i : value, length);
          if (publisher->send(reinterpret_cast<char *>(bytes.data()), bytes.size()) != static_cast<int>(bytes.size())) return 14;
        }
      }
    } else if (command == "empty") {
      std::unique_ptr<Message> message(subscriber->receive(true));
      if (message) return 15;
    } else if (command == "readers") {
      for (int i = 0; i < 40; ++i) {
        std::unique_ptr<SubSocket> temporary(SubSocket::create(context.get(), "nativeToRust", "127.0.0.1", false, false, 4096));
        if (!temporary) return 16;
      }
    } else if (command == "restart") {
      publisher.reset(PubSocket::create(context.get(), "nativeToRust", false, 4096));
      if (!publisher) return 17;
    } else { return 18; }
    std::cout << "OK" << std::endl;
  }
  return 19;
}

int main(int argc, char **argv) {
  if (argc >= 2 && std::string(argv[1]) == "--scripted") return scripted(argc == 3 && std::string(argv[2]) == "conflate");
  std::unique_ptr<Context> context(Context::create());
  std::unique_ptr<SubSocket> subscriber(SubSocket::create(context.get(), "rustToNative"));
  std::unique_ptr<PubSocket> publisher(PubSocket::create(context.get(), "nativeToRust"));
  if (!subscriber || !publisher) return 1;
  subscriber->setTimeout(2000);
  std::cout << "READY" << std::endl;
  std::unique_ptr<Message> message(subscriber->receive());
  if (!message) return 2;
  if (argc == 2 && std::string(argv[1]) == "--delayed-reply") {
    std::this_thread::sleep_for(std::chrono::milliseconds(2100));
  }
  if (publisher->sendMessage(message.get()) < 0) return 3;
  std::cout << "SENT" << std::endl;
  return 0;
}
