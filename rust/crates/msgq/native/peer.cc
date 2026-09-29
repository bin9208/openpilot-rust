#include "msgq/ipc.h"
#include <iostream>
#include <memory>

int main() {
  std::unique_ptr<Context> context(Context::create());
  std::unique_ptr<SubSocket> subscriber(SubSocket::create(context.get(), "rustToNative"));
  std::unique_ptr<PubSocket> publisher(PubSocket::create(context.get(), "nativeToRust"));
  if (!subscriber || !publisher) return 1;
  subscriber->setTimeout(2000);
  std::cout << "READY" << std::endl;
  std::unique_ptr<Message> message(subscriber->receive());
  if (!message) return 2;
  if (publisher->sendMessage(message.get()) < 0) return 3;
  return 0;
}
