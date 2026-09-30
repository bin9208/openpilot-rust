// Oracle driver: links unchanged common/swaglog.cc and uv.lock's json11 library.
#include <atomic>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <string>
#include <thread>
#include <vector>
#include <zmq.h>
#include "common/swaglog.h"
#include "json11/json11.hpp"

uint64_t fixture_clock = 0;
static std::atomic<int> sent{0}, dropped{0}, other_errors{0}, linger{-1};
extern "C" int __real_zmq_send(void*, const void*, size_t, int);
extern "C" int __real_zmq_setsockopt(void*, int, const void*, size_t);
extern "C" int __wrap_zmq_send(void* socket, const void* data, size_t size, int flags) {
  if (flags != ZMQ_NOBLOCK) std::abort();
  int result = __real_zmq_send(socket, data, size, flags);
  if (result >= 0) ++sent;
  else if (errno == EAGAIN) ++dropped;
  else ++other_errors;
  return result;
}
extern "C" int __wrap_zmq_setsockopt(void* socket, int option, const void* data, size_t size) {
  if (option == ZMQ_LINGER) linger = *static_cast<const int*>(data);
  return __real_zmq_setsockopt(socket, option, data, size);
}
static void emit(int level, const std::string& text) {
  if (text.find('\0') == std::string::npos) cloudlog(level, "%s", text.c_str());
  else cloudlog(level, "%s%c%s", text.substr(0, text.find('\0')).c_str(), 0, "tail");
}
static void rate(const std::string& text) { cloudlog_rl(2, 100, CLOUDLOG_INFO, "%s", text.c_str()); }
int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    std::string error;
    auto command = json11::Json::parse(line, error);
    if (!error.empty()) return 2;
    int before_sent = sent, before_dropped = dropped;
    auto op = command["op"].string_value();
    if (op == "emit") emit(command["level"].int_value(), command["text"].string_value());
    else if (op == "rate") {
      fixture_clock = std::stoull(command["timestamp"].string_value());
      rate(command["text"].string_value());
    } else if (op == "flood") {
      for (int i = 0; i < command["count"].int_value(); ++i) emit(CLOUDLOG_DEBUG, "flood");
    } else if (op == "threads") {
      std::vector<std::thread> workers;
      for (int i = 0; i < command["count"].int_value(); ++i) workers.emplace_back([i] { emit(CLOUDLOG_INFO, "thread-" + std::to_string(i)); });
      for (auto& thread : workers) thread.join();
    } else return 3;
    fflush(stdout);
    auto response = json11::Json::object{{"sent", sent - before_sent}, {"dropped", dropped - before_dropped}, {"linger", linger.load()}, {"errors", other_errors.load()}};
    std::cerr << json11::Json(response).dump() << std::endl;
  }
}
