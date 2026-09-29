#pragma once
#include <memory>
#include "rust/cxx.h"
#include "msgq/msgq.h"

namespace openpilot_rust {
class PublisherLock;
class Queue final {
public:
  Queue(const std::string &endpoint, bool publisher, bool conflate, size_t capacity, const std::string &lock_path);
  ~Queue();
  Queue(const Queue &) = delete;
  Queue &operator=(const Queue &) = delete;
  void send(rust::Slice<const uint8_t> bytes);
  bool readers_caught_up();
  rust::Vec<uint8_t> receive(int32_t timeout_ms);
private:
  msgq_queue_t queue_{};
  bool publisher_;
  std::unique_ptr<PublisherLock> lock_;
};
std::unique_ptr<Queue> open_queue(rust::Str endpoint, bool publisher, bool conflate, size_t capacity);
}
