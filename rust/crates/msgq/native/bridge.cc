#include "bridge.h"
#include "openpilot-msgq/src/bridge.rs.h"
#include <algorithm>
#include <cerrno>
#include <cstdlib>
#include <stdexcept>
#include <system_error>
#include <set>
#include <sys/stat.h>
#include <sys/file.h>
#include <fcntl.h>
#include <unistd.h>

namespace openpilot_rust {
class PublisherLock final {
public:
  explicit PublisherLock(const std::string &path) {
    fd_ = ::open(path.c_str(), O_RDWR | O_CREAT | O_CLOEXEC, 0600);
    if (fd_ < 0) throw std::system_error(errno, std::generic_category(), "open publisher lock");
    if (::flock(fd_, LOCK_EX | LOCK_NB) != 0) {
      const int error = errno;
      ::close(fd_);
      throw std::system_error(error, std::generic_category(), "another Rust publisher owns this endpoint");
    }
  }
  ~PublisherLock() { ::close(fd_); }
  PublisherLock(const PublisherLock &) = delete;
  PublisherLock &operator=(const PublisherLock &) = delete;
private:
  int fd_ = -1;
};

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

Queue::Queue(const std::string &endpoint, bool publisher, bool conflate, size_t capacity, const std::string &lock_path) : publisher_(publisher) {
  if (publisher) lock_ = std::make_unique<PublisherLock>(lock_path);
  if (msgq_new_queue(&queue_, endpoint.c_str(), capacity) != 0) {
    throw std::system_error(errno, std::generic_category(), "open msgq");
  }
  if (publisher) msgq_init_publisher(&queue_);
  else msgq_init_subscriber(&queue_);
  queue_.read_conflate = conflate;
}

Queue::~Queue() { msgq_close_queue(&queue_); }

bool Queue::readers_caught_up() { return publisher_ && msgq_all_readers_updated(&queue_); }

void Queue::send(rust::Slice<const uint8_t> bytes) {
  if (!publisher_ || bytes.empty() || bytes.size() > queue_.size / 3 - 16) {
    throw std::invalid_argument("send requires a publisher and a nonempty payload fitting one third of the queue");
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

static std::unique_ptr<Queue> open_checked_queue(rust::Str endpoint, bool publisher, bool conflate, size_t capacity, bool isolated) {
  const char *raw_prefix = std::getenv("OPENPILOT_PREFIX");
  const std::string prefix = raw_prefix ? raw_prefix : "";
  if (isolated && (!component(prefix) || prefix.rfind("rust-probe-", 0) != 0 || prefix.size() <= 11)) {
    throw std::invalid_argument("OPENPILOT_PREFIX must be an isolated rust-probe-NAME namespace");
  }
  if (!prefix.empty() && !component(prefix)) throw std::invalid_argument("invalid runtime namespace");
  const std::string name(endpoint);
  if (!component(name)) throw std::invalid_argument("invalid endpoint");
  if (std::getenv("CEREAL_FAKE")) throw std::invalid_argument("CEREAL_FAKE is unsupported");
  if (capacity < 4096 || capacity > 64 * 1024 * 1024 || capacity % 8 != 0) {
    throw std::invalid_argument("queue capacity must be 4096..67108864 bytes and aligned to 8 bytes");
  }
  const std::string path = "/dev/shm/msgq_" + (raw_prefix ? prefix + "/" : "") + name;
  struct stat info{};
  if (::stat(path.c_str(), &info) == 0) {
    if (!S_ISREG(info.st_mode) || info.st_size != static_cast<off_t>(capacity + sizeof(msgq_header_t))) {
      throw std::invalid_argument("existing msgq queue has incompatible size or type");
    }
  } else if (errno != ENOENT) {
    throw std::system_error(errno, std::generic_category(), "inspect msgq");
  }
  return std::make_unique<Queue>(name, publisher, conflate, capacity, path + ".rust-publisher-lock");
}

std::unique_ptr<Queue> open_queue(rust::Str endpoint, bool publisher, bool conflate, size_t capacity) {
  return open_checked_queue(endpoint, publisher, conflate, capacity, true);
}

std::unique_ptr<Queue> open_runtime_queue(rust::Str endpoint, bool publisher, bool conflate, size_t capacity) {
  return open_checked_queue(endpoint, publisher, conflate, capacity, false);
}

QueueBatch::QueueBatch(rust::Slice<const QueueSpec> specifications, bool isolated) {
  if (specifications.empty() || specifications.size() > 256) throw std::invalid_argument("invalid subscription count");
  std::set<std::string> names;
  for (const auto &specification : specifications) {
    if (!names.insert(std::string(specification.endpoint)).second) throw std::invalid_argument("duplicate subscription");
    queues_.push_back(open_checked_queue(specification.endpoint, false, true, specification.capacity, isolated));
    const size_t index = queues_.size() - 1;
    if (specification.polled) {
      polls_.push_back(msgq_pollitem_t{&queues_.back()->queue_, 0});
      poll_indices_.push_back(index);
    } else {
      unpolled_indices_.push_back(index);
    }
  }
  if (polls_.empty()) throw std::invalid_argument("at least one polled subscription is required");
}

rust::Vec<QueuedMessage> QueueBatch::receive(int32_t timeout_ms) {
  if (timeout_ms < 0) throw std::invalid_argument("invalid poll timeout");
  msgq_poll(polls_.data(), polls_.size(), timeout_ms);
  rust::Vec<QueuedMessage> result;
  const auto receive = [&](size_t index) {
    auto bytes = queues_[index]->receive(0);
    if (!bytes.empty()) result.push_back(QueuedMessage{index, std::move(bytes)});
  };
  for (size_t index = 0; index < polls_.size(); ++index) {
    if (polls_[index].revents) receive(poll_indices_[index]);
  }
  for (const size_t index : unpolled_indices_) receive(index);
  return result;
}

std::unique_ptr<QueueBatch> open_batch(rust::Slice<const QueueSpec> specifications, bool isolated) {
  return std::make_unique<QueueBatch>(specifications, isolated);
}
}
