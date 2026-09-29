#include "bridge.h"
#include <algorithm>
#include <cerrno>
#include <cstdlib>
#include <stdexcept>
#include <system_error>

namespace openpilot_rust {
namespace {
bool component(const std::string &value) {
  return !value.empty() && value.size() <= 100 &&
      std::all_of(value.begin(), value.end(), [](unsigned char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
               (c >= '0' && c <= '9') || c == '_' || c == '-';
      });
}
struct Received {
  msgq_msg_t message{};
  ~Received() { msgq_msg_close(&message); }
};
}

Queue::Queue(const std::string &endpoint, bool publisher, bool conflate) : publisher_(publisher) {
  if (msgq_new_queue(&queue_, endpoint.c_str(), DEFAULT_SEGMENT_SIZE) != 0) {
    throw std::system_error(errno, std::generic_category(), "open msgq");
  }
  if (publisher) msgq_init_publisher(&queue_);
  else msgq_init_subscriber(&queue_);
  queue_.read_conflate = conflate;
}

Queue::~Queue() { msgq_close_queue(&queue_); }

void Queue::send(rust::Slice<const uint8_t> bytes) {
  if (!publisher_ || bytes.empty() || bytes.size() > DEFAULT_SEGMENT_SIZE / 3 - 16) {
    throw std::invalid_argument("send requires a publisher and 1..349509 bytes");
  }
  msgq_msg_t message{bytes.size(), const_cast<char *>(reinterpret_cast<const char *>(bytes.data()))};
  if (msgq_msg_send(&message, &queue_) < 0) {
    throw std::system_error(errno, std::generic_category(), "send msgq");
  }
}

rust::Vec<uint8_t> Queue::receive(int32_t timeout_ms) {
  if (publisher_ || timeout_ms < 0) throw std::invalid_argument("invalid receive");
  Received received;
  int result = msgq_msg_recv(&received.message, &queue_);
  if (result == 0 && timeout_ms > 0) {
    msgq_pollitem_t item{&queue_, 0};
    msgq_poll(&item, 1, timeout_ms);
    result = msgq_msg_recv(&received.message, &queue_);
  }
  if (result < 0) throw std::system_error(errno, std::generic_category(), "receive msgq");
  rust::Vec<uint8_t> bytes;
  if (result > 0) {
    bytes.reserve(received.message.size);
    for (size_t i = 0; i < received.message.size; ++i) {
      bytes.push_back(static_cast<uint8_t>(received.message.data[i]));
    }
  }
  return bytes;
}

std::unique_ptr<Queue> open_queue(rust::Str endpoint, bool publisher, bool conflate) {
  const char *raw_prefix = std::getenv("OPENPILOT_PREFIX");
  const std::string prefix = raw_prefix ? raw_prefix : "";
  if (!component(prefix) || prefix.rfind("rust-probe-", 0) != 0 || prefix.size() <= 11) {
    throw std::invalid_argument("OPENPILOT_PREFIX must be an isolated rust-probe-NAME namespace");
  }
  const std::string name(endpoint);
  if (!component(name)) throw std::invalid_argument("invalid endpoint");
  if (std::getenv("CEREAL_FAKE")) throw std::invalid_argument("CEREAL_FAKE is unsupported");
  return std::make_unique<Queue>(name, publisher, conflate);
}
}
