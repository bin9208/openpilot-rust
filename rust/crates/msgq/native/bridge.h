#pragma once
#include <memory>
#include <string>
#include <vector>
#include "rust/cxx.h"
#include "msgq/msgq.h"

namespace openpilot_rust {
struct QueueSpec;
struct QueuedMessage;
class PublisherLock;
class Queue final {
public:
  Queue(const std::string &endpoint, bool publisher, bool conflate, size_t capacity, const std::string &lock_path);
  ~Queue();
  Queue(const Queue &) = delete;
  Queue &operator=(const Queue &) = delete;
  void send(rust::Slice<const uint8_t> bytes);
  bool send_if_current(rust::Slice<const uint8_t> bytes);
  bool readers_caught_up();
  rust::Vec<uint8_t> receive(int32_t timeout_ms);
private:
  friend class QueueBatch;
  msgq_queue_t queue_{};
  bool publisher_;
  std::unique_ptr<PublisherLock> lock_;
};
std::unique_ptr<Queue> open_queue(rust::Str endpoint, bool publisher, bool conflate, size_t capacity);
std::unique_ptr<Queue> open_runtime_queue(rust::Str endpoint, bool publisher, bool conflate, size_t capacity);
std::unique_ptr<Queue> open_transient_runtime_publisher(rust::Str endpoint, size_t capacity);
class QueueBatch final {
public:
  QueueBatch(rust::Slice<const QueueSpec> specifications, bool isolated, bool conflate, bool lazy = false);
  rust::Vec<QueuedMessage> receive(int32_t timeout_ms);
  rust::Vec<size_t> poll_ready(int32_t timeout_ms);
  rust::Vec<uint8_t> receive_one(size_t index);
  void set_active(size_t index, bool active);
private:
  struct Endpoint { std::string name; size_t capacity; bool polled; };
  std::vector<Endpoint> endpoints_;
  bool lazy_;
  std::vector<std::unique_ptr<Queue>> queues_;
  std::vector<msgq_pollitem_t> polls_;
  std::vector<size_t> poll_indices_;
  std::vector<size_t> unpolled_indices_;
};
std::unique_ptr<QueueBatch> open_batch(rust::Slice<const QueueSpec> specifications, bool isolated);
std::unique_ptr<QueueBatch> open_queued_batch(rust::Slice<const QueueSpec> specifications);
std::unique_ptr<QueueBatch> open_lazy_batch(rust::Slice<const QueueSpec> specifications);
}
