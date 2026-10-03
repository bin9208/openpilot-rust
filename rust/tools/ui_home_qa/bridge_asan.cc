#include "bridge.h"
#include "openpilot-startup-ui/src/bridge.rs.h"
#include <future>
#include <iostream>
#include <string>
namespace startup_ui { void trace_log(int32_t, rust::Slice<const uint8_t>) noexcept {} }
struct Decoded { int32_t width; int32_t height; rust::Vec<uint8_t> bytes; };
int main(int argc,char **argv) {
  if(argc!=3)return 2;
  for(int cycle=0;cycle<4;++cycle){
    auto surface=startup_ui::create(536,240,"training ownership ASAN",32);
    for(int step=0;step<19;++step){
      const auto path=std::string(argv[1])+"/step"+std::to_string(step)+".png";
      auto worker=std::async(std::launch::async,[path]{auto image=startup_ui::image(path);return Decoded{image->width(),image->height(),image->rgba()};});
      auto image=worker.get();
      if(image.bytes.size()!=uint64_t(image.width)*uint64_t(image.height)*4)return 3;
      const auto texture=surface->pixel_texture(image.width,image.height,{image.bytes.data(),image.bytes.size()});
      surface->smooth_texture(texture);
      image.bytes.clear();
      surface->begin(1);
      surface->clear(0xff000000);
      surface->tinted_texture(texture,{0,0,float(image.width),float(image.height)},{0,0,536,240},{0,0},0,0xffffffff);
      surface->ring({{468,68},53,61,90,float(90+step*20),36,0xff40ff00});
      surface->screenshot(argv[2]);
      surface->finish_content(1);surface->present();
      surface->texture_release(texture);
      surface->texture_release(texture);
    }
  }
  std::cout<<"PASS 4 create/destroy cycles, 76 worker CPU decodes, 76 filtered GPU uploads/draws/releases and duplicate-release guards, 76 native rings\n";
}
