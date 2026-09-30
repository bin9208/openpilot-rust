#include "kernel.h"
#include <cassert>
#include <fcntl.h>
#include <iostream>
#include <stdexcept>
#include <unistd.h>
int main(int argc,char **argv){
  assert(argc==3);
  sensord_kernel::realtime(false);
  const int fd=open(argv[1],O_RDWR|O_CLOEXEC);assert(fd>=0);
  assert(sensord_kernel::read_byte(fd,0x6a,0x0f,false)==0x6a);
  sensord_kernel::write_byte(fd,0x6a,0x60,255,true);
  assert(sensord_kernel::read_byte(fd,0x6a,0x60,false)==255);
  for(auto reg:{0x40,0x41}){auto data=sensord_kernel::read_block(fd,0x6a,reg,6,false);assert(data->size()==6&&data->at(5)==6);}
  assert(sensord_kernel::read_block(fd,0x6a,0x42,6,false)->size()==2);
  assert(sensord_kernel::read_block(fd,0x6a,0x43,32,false)->at(31)==32);
  assert(sensord_kernel::read_block(fd,0x6a,0x44,0,false)->empty());
  bool invalid=false;try{sensord_kernel::read_block(fd,0x6a,0x44,33,false);}catch(const std::invalid_argument&){invalid=true;}assert(invalid);
  bool interrupted=false;try{sensord_kernel::read_block(fd,0x6a,0xee,6,false);}catch(const std::exception&){interrupted=true;}assert(interrupted);
  auto gpio=sensord_kernel::open_gpio(argv[2],"sensord",84);
  assert(gpio->poll_event(100)&3);auto bytes=gpio->read_events();assert(bytes->size()==32);gpio.reset();
  assert(close(fd)==0);
  std::cout<<"PASS native SMBus/GPIO/scheduling boundary under ASan+UBSan\n";
}
