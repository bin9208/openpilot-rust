#include "msgq/ipc.h"
#include <iostream>
#include <memory>
#include <chrono>
#include <string>
#include <thread>

int main(int argc, char **argv) {
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
