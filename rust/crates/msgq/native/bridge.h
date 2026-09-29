#pragma once
#include <memory>
#include "rust/cxx.h"
#include "msgq/msgq.h"

namespace openpilot_rust {
class Queue final {
public:
  Queue(const std::string &endpoint, bool publisher, bool conflate);
  ~Queue();
  Queue(const Queue &) = delete;
  Queue &operator=(const Queue &) = delete;
  void send(rust::Slice<const uint8_t> bytes);
  rust::Vec<uint8_t> receive(int32_t timeout_ms);
private:
  msgq_queue_t queue_{};
  bool publisher_;
};
std::unique_ptr<Queue> open_queue(rust::Str endpoint, bool publisher, bool conflate);
}
