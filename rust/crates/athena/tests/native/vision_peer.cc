#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdlib>
#include <iostream>
#include <poll.h>
#include <string>
#include <thread>
#include <unistd.h>
#include "msgq/visionipc/visionipc_server.h"
static std::atomic<bool> running{true};
static_assert(std::atomic<bool>::is_always_lock_free);
static void stop(int) { running.store(false,std::memory_order_relaxed); }
int main() {
  std::signal(SIGINT,stop);
  std::signal(SIGTERM,stop);
  VisionIpcServer server("camerad");
  for (auto stream : {VISION_STREAM_WIDE_ROAD,VISION_STREAM_DRIVER}) server.create_buffers_with_sizes(stream,4,8,4,320,16,64);
  server.start_listener();
  std::atomic<unsigned> frame{80};
  std::thread producer([&] {
    while (running.load(std::memory_order_relaxed)) {
      const auto id=frame.load(std::memory_order_relaxed);
      for (auto stream : {VISION_STREAM_WIDE_ROAD,VISION_STREAM_DRIVER}) {
        auto *buffer=server.get_buffer(stream);
        for (size_t i=0;i<buffer->len;++i) static_cast<unsigned char*>(buffer->addr)[i]=static_cast<unsigned char>(i);
        buffer->set_frame_id(id);
        VisionIpcBufExtra extra{id,uint64_t{id}*1000,uint64_t{id}*1000+100,true};
        server.send(buffer,&extra);
      }
      std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
  });
  std::cout << "READY " << getpid() << std::endl;
  while (running.load(std::memory_order_relaxed)) {
    if (std::getenv("ATHENA_VISION_FIXTURE_AUTO")) { std::this_thread::sleep_for(std::chrono::milliseconds(20));continue; }
    pollfd input{STDIN_FILENO,POLLIN,0};
    if (poll(&input,1,100)<=0) continue;
    std::string command;
    if (!(std::cin>>command) || command=="stop") { running.store(false,std::memory_order_relaxed);break; }
    if (command=="frame") { unsigned id;std::cin>>id;frame.store(id,std::memory_order_relaxed); }
    else { running.store(false,std::memory_order_relaxed);break; }
    std::cout << "OK" << std::endl;
  }
  producer.join();
}
