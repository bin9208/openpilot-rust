#include <algorithm>
#include <array>
#include <cerrno>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <dlfcn.h>
#include <fcntl.h>
#include <map>
#include <mutex>
#include <poll.h>
#include <string>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <unistd.h>
#include "third_party/linux/include/v4l2-controls.h"
#include <linux/videodev2.h>
#include <linux/ion.h>
#include <linux/msm_ion.h>
#include "vendor_events.h"

struct Slot {unsigned index;unsigned long address;unsigned capacity;unsigned used;int fd;timeval timestamp;};
struct Device {
  std::deque<Slot> capture,input,returned;
  std::deque<v4l2_event> events;
  unsigned packets=0,frames=0,width=96,height=64;
  bool port=false,poll_interrupted=false,ioctl_interrupted=false;
};
struct State {
  std::mutex lock;
  std::map<int,Device> devices;
  std::map<int,bool> ions;
  std::map<int,std::pair<int,size_t>> allocations;
  int handle=1;
};
static State &state(){static auto *value=new State;return *value;}
static std::string scenario(){const auto value=getenv("VIDC_FAKE_CASE");return value?value:"normal";}
static void record(const char *format,...) {
  const auto path=getenv("VIDC_FAKE_TRACE");if(!path)return;
  auto file=fopen(path,"a");if(!file)abort();
  va_list args;va_start(args,format);vfprintf(file,format,args);va_end(args);fputc('\n',file);fclose(file);
}
static int reject(int error=EINVAL){errno=error;return -1;}
static int device_open(const char *path) {
  if(strcmp(path,"/dev/ion") && strcmp(path,"/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index0"))return -2;
  std::lock_guard guard(state().lock);
  int fd=syscall(SYS_memfd_create,"vidc-driver-fixture",0);if(fd<0)abort();
  if(!strcmp(path,"/dev/ion")){state().ions[fd]=true;record("OPEN ion");}
  else{state().devices[fd]=Device{};record("OPEN v4l");}
  return fd;
}
extern "C" int open(const char *path,int flags,...) {
  int fd=device_open(path);if(fd!=-2)return fd;
  mode_t mode=0;if(flags&O_CREAT){va_list args;va_start(args,flags);mode=va_arg(args,int);va_end(args);}
  static auto next=reinterpret_cast<int(*)(const char *,int,...)>(dlsym(RTLD_NEXT,"open"));return next(path,flags,mode);
}
extern "C" int open64(const char *path,int flags,...) {
  mode_t mode=0;if(flags&O_CREAT){va_list args;va_start(args,flags);mode=va_arg(args,int);va_end(args);}return open(path,flags,mode);
}
extern "C" int openat(int directory,const char *path,int flags,...) {
  int fd=device_open(path);if(fd!=-2)return fd;
  mode_t mode=0;if(flags&O_CREAT){va_list args;va_start(args,flags);mode=va_arg(args,int);va_end(args);}return syscall(SYS_openat,directory,path,flags,mode);
}
extern "C" int openat64(int directory,const char *path,int flags,...) {
  mode_t mode=0;if(flags&O_CREAT){va_list args;va_start(args,flags);mode=va_arg(args,int);va_end(args);}return openat(directory,path,flags,mode);
}
extern "C" int close(int fd) {
  {std::lock_guard guard(state().lock);if(state().devices.erase(fd))record("CLOSE v4l");if(state().ions.erase(fd))record("CLOSE ion");}
  return syscall(SYS_close,fd);
}
extern "C" int ioctl(int fd,unsigned long request,...) {
  va_list args;va_start(args,request);void *argument=va_arg(args,void *);va_end(args);
  std::unique_lock guard(state().lock);
  if(state().ions.count(fd)) {
    if(request==ION_IOC_ALLOC) {
      auto value=static_cast<ion_allocation_data *>(argument);
      if(value->align!=4096 || value->heap_id_mask!=(1U<<ION_IOMMU_HEAP_ID) || value->flags!=ION_FLAG_CACHED)abort();
      int allocation=syscall(SYS_memfd_create,"vidc-pixels",0);if(allocation<0 || ftruncate(allocation,value->len))abort();
      value->handle=state().handle++;state().allocations[value->handle]={allocation,value->len};record("ION_ALLOC %zu",value->len);return 0;
    }
    if(request==ION_IOC_SHARE) {auto value=static_cast<ion_fd_data *>(argument);value->fd=dup(state().allocations.at(value->handle).first);record("ION_SHARE");return 0;}
    if(request==ION_IOC_FREE) {auto value=static_cast<ion_handle_data *>(argument);syscall(SYS_close,state().allocations.at(value->handle).first);state().allocations.erase(value->handle);record("ION_FREE");return 0;}
    if(request==ION_IOC_CUSTOM) {
      auto value=static_cast<ion_custom_data *>(argument);auto flush=reinterpret_cast<ion_flush_data *>(value->arg);
      auto allocation=state().allocations.at(flush->handle);
      if((value->cmd!=ION_IOC_CLEAN_CACHES && value->cmd!=ION_IOC_INV_CACHES) || !flush->vaddr || flush->offset || flush->fd || flush->length+8!=allocation.second)abort();
      record("ION_SYNC %s %u",value->cmd==ION_IOC_CLEAN_CACHES?"to":"from",flush->length);return 0;
    }
    record("UNKNOWN_ION %lu",request);abort();
  }
  auto found=state().devices.find(fd);
  if(found==state().devices.end()) {guard.unlock();static auto next=reinterpret_cast<int(*)(int,unsigned long,...)>(dlsym(RTLD_NEXT,"ioctl"));return next(fd,request,argument);}
  auto &device=found->second;auto test=scenario();
  if(request==VIDIOC_QUERYCAP) {
    if(test=="ioctl-eintr" && !device.ioctl_interrupted){device.ioctl_interrupted=true;record("IOCTL_EINTR");return reject(EINTR);}
    auto value=static_cast<v4l2_capability *>(argument);memset(value,0,sizeof(*value));strcpy(reinterpret_cast<char *>(value->driver),test=="other-capability"?"other":"msm_vidc_driver");strcpy(reinterpret_cast<char *>(value->card),"msm_vidc_vdec");record("QUERYCAP");return 0;
  }
  if(request==VIDIOC_SUBSCRIBE_EVENT) {record("SUBSCRIBE %u",static_cast<v4l2_event_subscription *>(argument)->type);return 0;}
  if(request==VIDIOC_S_FMT || request==VIDIOC_G_FMT) {
    auto value=static_cast<v4l2_format *>(argument);auto &format=value->fmt.pix_mp;
    if(request==VIDIOC_S_FMT){device.width=format.width;device.height=format.height;record("FORMAT %u %u %u %u",value->type,format.width,format.height,format.pixelformat);}
    else{format.width=device.width;format.height=device.height;record("GET_FORMAT %u",value->type);}
    format.plane_fmt[0].sizeimage=value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE?4096:8192;
    format.plane_fmt[0].bytesperline=test=="wide-stride"?256:128;
    if(test=="wrong-h264" && value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)format.pixelformat=V4L2_PIX_FMT_NV12;
    if(test=="zero-input-size" && value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)format.plane_fmt[0].sizeimage=0;
    if(test=="wrong-nv12" && value->type==V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)format.pixelformat=V4L2_PIX_FMT_H264;
    return 0;
  }
  if(request==VIDIOC_S_PARM){auto value=static_cast<v4l2_streamparm *>(argument);record("FPS %u %u %u",value->type,value->parm.output.timeperframe.numerator,value->parm.output.timeperframe.denominator);return 0;}
  if(request==VIDIOC_S_EXT_CTRLS){auto value=static_cast<v4l2_ext_controls *>(argument);if(value->count!=2 || value->ctrl_class!=V4L2_CTRL_CLASS_MPEG)abort();for(unsigned index=0;index<value->count;++index)record("CONTROL %u %d",value->controls[index].id,value->controls[index].value);return 0;}
  if(request==VIDIOC_G_CTRL){auto value=static_cast<v4l2_control *>(argument);record("MIN_CAPTURE %u",value->id);if(test=="min-control-eintr")return reject(EINTR);value->value=test=="min-low"?2:test=="min-high"?50:12;return 0;}
  if(request==VIDIOC_REQBUFS){auto value=static_cast<v4l2_requestbuffers *>(argument);if(value->memory!=V4L2_MEMORY_USERPTR)abort();record("BUFFERS %u %u",value->type,value->count);if(value->count>0){if(test=="zero-buffers")value->count=0;else if(test=="fewer-buffers")value->count=std::min(3U,value->count);else if(test=="extra-buffers")value->count+=2;}else if(value->type==V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)device.capture.clear();return 0;}
  if(request==VIDIOC_STREAMON || request==VIDIOC_STREAMOFF){record("STREAM %d %u",request==VIDIOC_STREAMON,*static_cast<unsigned *>(argument));return 0;}
  if(request==VIDIOC_QBUF) {
    auto value=static_cast<v4l2_buffer *>(argument);auto plane=value->m.planes;
    if(value->memory!=V4L2_MEMORY_USERPTR || value->length!=1 || !plane->m.userptr || !plane->length || plane->bytesused>plane->length || fcntl(plane->reserved[0],F_GETFD)<0)abort();
    Slot slot{value->index,plane->m.userptr,plane->length,plane->bytesused,int(plane->reserved[0]),value->timestamp};
    if(value->type==V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE){if(value->flags || slot.used!=slot.capacity)abort();device.capture.push_back(slot);record("QUEUE_CAPTURE %u %u",slot.index,slot.capacity);}
    else{if(value->flags!=V4L2_BUF_FLAG_TIMESTAMP_COPY)abort();device.input.push_back(slot);device.packets++;record("QUEUE_INPUT %u %u %u %ld %ld",slot.index,slot.used,slot.capacity,slot.timestamp.tv_sec,slot.timestamp.tv_usec);if(test=="queue-failure")return reject();}
    return 0;
  }
  if(request==VIDIOC_DQBUF) {
    auto value=static_cast<v4l2_buffer *>(argument);auto plane=value->m.planes;
    if(value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE){if(device.returned.empty())return reject(EAGAIN);auto slot=device.returned.front();device.returned.pop_front();value->index=test=="wrong-input-index"?100:slot.index;record("RETURN_INPUT %u",value->index);return 0;}
    if(device.capture.empty() || device.input.empty())return reject(test=="enoent"?ENOENT:EAGAIN);
    auto capture=device.capture.front();device.capture.pop_front();auto input=device.input.front();device.input.pop_front();device.returned.push_back(input);
    if(fcntl(capture.fd,F_GETFD)<0 || fcntl(input.fd,F_GETFD)<0)abort();
    value->index=test=="wrong-capture-index"?100:capture.index;value->timestamp=input.timestamp;if(test=="timestamp-zero")value->timestamp={};
    plane->bytesused=test=="empty-frame" && device.frames==0?0:capture.capacity;plane->data_offset=7;
    for(unsigned index=0;index<64;++index)reinterpret_cast<uint8_t *>(capture.address)[index]=uint8_t(index+device.frames*17+reinterpret_cast<uint8_t *>(input.address)[index%input.used]);
    device.frames++;record("FRAME %u %u %ld %ld",value->index,plane->bytesused,value->timestamp.tv_sec,value->timestamp.tv_usec);return 0;
  }
  if(request==VIDIOC_DQEVENT){if(device.events.empty())return reject(EAGAIN);auto value=static_cast<v4l2_event *>(argument);*value=device.events.front();device.events.pop_front();record("EVENT %u",value->type);return 0;}
  if(request==VIDIOC_DECODER_CMD){auto value=static_cast<v4l2_decoder_cmd *>(argument);if(value->cmd!=V4L2_QCOM_CMD_FLUSH || value->flags!=V4L2_QCOM_CMD_FLUSH_CAPTURE)abort();record("FLUSH_CAPTURE");v4l2_event event{};event.type=V4L2_EVENT_MSM_VIDC_FLUSH_DONE;memcpy(event.u.data,&value->flags,4);device.events.push_back(event);return 0;}
  record("UNKNOWN %lu",request);abort();
}
extern "C" int poll(pollfd *fds,nfds_t count,int timeout) {
  std::unique_lock guard(state().lock);
  if(count!=1 || !state().devices.count(fds[0].fd)){guard.unlock();static auto next=reinterpret_cast<int(*)(pollfd *,nfds_t,int)>(dlsym(RTLD_NEXT,"poll"));return next(fds,count,timeout);}
  auto &device=state().devices.at(fds[0].fd);auto test=scenario();
  if(test=="poll-eintr" && !device.poll_interrupted){device.poll_interrupted=true;record("POLL_EINTR");return reject(EINTR);}
  if(test=="timeout"){fds[0].revents=0;record("TIMEOUT");return 0;}
  if((test=="port-change" || test=="leased-port-change") && !device.port && device.packets>=(test=="leased-port-change"?2U:1U)) {
    device.port=true;v4l2_event event{};event.type=V4L2_EVENT_MSM_VIDC_PORT_SETTINGS_CHANGED_INSUFFICIENT;std::array<unsigned,2> dimensions{96,128};memcpy(event.u.data,dimensions.data(),8);device.events.push_back(event);
  }
  fds[0].revents=0;
  if(!device.events.empty())fds[0].revents|=POLLPRI;
  if(!device.returned.empty())fds[0].revents|=POLLOUT;
  if(!device.input.empty() && !device.capture.empty())fds[0].revents|=POLLIN;
  if(test=="poll-error"){fds[0].revents=POLLERR;record("POLL_ERROR");}
  if(fds[0].revents)return 1;
  record("TIMEOUT");return 0;
}
