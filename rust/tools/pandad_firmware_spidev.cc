#define _LARGEFILE64_SOURCE
#include <algorithm>
#include <cstdarg>
#include <cerrno>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <fcntl.h>
#include <fstream>
#include <map>
#include <string>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>
#include <linux/spi/spidev.h>
#include <json11/json11.hpp>

using json11::Json;
struct State {
  Json input;
  Json::array calls;
  std::map<int,int> handles;
  int next_id=0;
  uint32_t speed=50000000;
};
static State *state=nullptr;
template<class T> T original(const char *name) {
  void *symbol=dlsym(RTLD_NEXT,name);
  if (!symbol) std::abort();
  return reinterpret_cast<T>(symbol);
}
bool owned(const char *path) { return state && std::strcmp(path,"/dev/spidev0.0")==0; }
bool record(Json value) {
  state->calls.push_back(std::move(value));
  if (!state->input["fail_at"].is_null() && state->input["fail_at"].int_value()==static_cast<int>(state->calls.size())-1) {
    errno=state->input["error"].is_null() ? EIO : state->input["error"].int_value();
    return false;
  }
  return true;
}
Json bytes(const uint8_t *data,size_t size) {
  Json::array result;
  for (size_t i=0;i<size;++i) result.emplace_back(data[i]);
  return result;
}
void response(uint8_t *data,size_t size) {
  const auto &pattern=state->input["response"].array_items();
  for (size_t i=0;i<size;++i) data[i]=pattern.empty() ? 0 : pattern[i%pattern.size()].int_value();
}
template<class T> int exists(T *out) {
  if (!record(Json::array{"exists"})) return -1;
  if (state->input["missing"].bool_value()) { errno=ENOENT; return -1; }
  *out={}; out->st_mode=S_IFCHR|0600; out->st_nlink=1;
  return 0;
}
extern "C" {
int panda_spi_fixture_marker() { return state ? 175 : 0; }
int stat(const char *path,struct stat *out) noexcept {
  return owned(path) ? exists(out) : original<int(*)(const char *,struct stat *)>("stat")(path,out);
}
int stat64(const char *path,struct stat64 *out) noexcept {
  return owned(path) ? exists(out) : original<int(*)(const char *,struct stat64 *)>("stat64")(path,out);
}
int __xstat(int version,const char *path,struct stat *out) noexcept {
  return owned(path) ? exists(out) : original<int(*)(int,const char *,struct stat *)>("__xstat")(version,path,out);
}
int __xstat64(int version,const char *path,struct stat64 *out) noexcept {
  return owned(path) ? exists(out) : original<int(*)(int,const char *,struct stat64 *)>("__xstat64")(version,path,out);
}
int __fxstatat64(int version,int fd,const char *path,struct stat64 *out,int flags) noexcept {
  return owned(path) ? exists(out) : original<int(*)(int,int,const char *,struct stat64 *,int)>("__fxstatat64")(version,fd,path,out,flags);
}
int fstatat(int fd,const char *path,struct stat *out,int flags) noexcept {
  return owned(path) ? exists(out) : original<int(*)(int,const char *,struct stat *,int)>("fstatat")(fd,path,out,flags);
}
int fstatat64(int fd,const char *path,struct stat64 *out,int flags) noexcept {
  return owned(path) ? exists(out) : original<int(*)(int,const char *,struct stat64 *,int)>("fstatat64")(fd,path,out,flags);
}
int open(const char *path,int flags,...) {
  mode_t mode=0;
  if (flags&O_CREAT) { va_list args; va_start(args,flags); mode=va_arg(args,int); va_end(args); }
  auto function=original<int(*)(const char *,int,...)>("open");
  if (!owned(path)) return function(path,flags,mode);
  const int id=state->next_id++;
  if (!record(Json::array{"open",id,flags})) return -1;
  const int fd=function("/dev/null",O_RDWR);
  if (fd>=0) state->handles[fd]=id;
  return fd;
}
int open64(const char *path,int flags,...) {
  mode_t mode=0;
  if (flags&O_CREAT) { va_list args; va_start(args,flags); mode=va_arg(args,int); va_end(args); }
  if (owned(path)) return open(path,flags,mode);
  return original<int(*)(const char *,int,...)>("open64")(path,flags,mode);
}
int close(int fd) {
  auto function=original<int(*)(int)>("close");
  if (!state || !state->handles.count(fd)) return function(fd);
  if (!record(Json::array{"close",state->handles.at(fd)})) return -1;
  state->handles.erase(fd);
  return function(fd);
}
int flock(int fd,int operation) noexcept {
  if (!state || !state->handles.count(fd)) return original<int(*)(int,int)>("flock")(fd,operation);
  return record(Json::array{"flock",state->handles.at(fd),operation}) ? 0 : -1;
}
ssize_t read(int fd,void *data,size_t length) {
  if (!state || !state->handles.count(fd)) return original<ssize_t(*)(int,void *,size_t)>("read")(fd,data,length);
  if (!record(Json::array{"read",state->handles.at(fd),static_cast<int>(length)})) return -1;
  const size_t count=state->input["read_result"].is_null() ? length : std::min(length,static_cast<size_t>(state->input["read_result"].int_value()));
  response(static_cast<uint8_t *>(data),count);
  return count;
}
ssize_t __read_chk(int fd,void *data,size_t length,size_t capacity) {
  if (!state || !state->handles.count(fd)) return original<ssize_t(*)(int,void *,size_t,size_t)>("__read_chk")(fd,data,length,capacity);
  if (length>capacity) std::abort();
  return read(fd,data,length);
}
ssize_t write(int fd,const void *data,size_t length) {
  if (!state || !state->handles.count(fd)) return original<ssize_t(*)(int,const void *,size_t)>("write")(fd,data,length);
  if (!record(Json::array{"write",state->handles.at(fd),bytes(static_cast<const uint8_t *>(data),length)})) return -1;
  return state->input["write_result"].is_null() ? length : static_cast<size_t>(state->input["write_result"].int_value());
}
int ioctl(int fd,unsigned long request,...) noexcept {
  va_list args; va_start(args,request); void *argument=va_arg(args,void *); va_end(args);
  if (!state || !state->handles.count(fd)) return original<int(*)(int,unsigned long,...)>("ioctl")(fd,request,argument);
  const int id=state->handles.at(fd);
  if (request==SPI_IOC_RD_MODE || request==SPI_IOC_RD_BITS_PER_WORD || request==SPI_IOC_RD_MAX_SPEED_HZ) {
    const char *name=request==SPI_IOC_RD_MODE ? "mode" : (request==SPI_IOC_RD_BITS_PER_WORD ? "bits" : "speed");
    if (!record(Json::array{"get",id,name})) return -1;
    if (request==SPI_IOC_RD_MAX_SPEED_HZ) *static_cast<uint32_t *>(argument)=state->speed;
    else *static_cast<uint8_t *>(argument)=request==SPI_IOC_RD_MODE ? state->input["mode"].int_value() : 8;
    return 0;
  }
  if (request==SPI_IOC_WR_MAX_SPEED_HZ) {
    const uint32_t speed=*static_cast<uint32_t *>(argument);
    if (!record(Json::array{"set_speed",id,static_cast<double>(speed)})) return -1;
    state->speed=speed; return 0;
  }
  if (request==SPI_IOC_MESSAGE(1)) {
    auto *transfer=static_cast<spi_ioc_transfer *>(argument);
    const auto *tx=reinterpret_cast<const uint8_t *>(transfer->tx_buf);
    auto *rx=reinterpret_cast<uint8_t *>(transfer->rx_buf);
    if (transfer->len>4096) std::abort();
    if (!record(Json::array{"transfer",id,bytes(tx,transfer->len),static_cast<double>(transfer->speed_hz),transfer->bits_per_word,
                           transfer->delay_usecs,transfer->cs_change,transfer->tx_nbits,transfer->rx_nbits})) return -1;
    response(rx,transfer->len); return transfer->len;
  }
  if (request==SPI_IOC_RD_LSB_FIRST) {
    struct Packet { uint64_t rx,tx; uint32_t tx_length,rx_max,timeout; uint8_t endpoint,disconnect; };
    static_assert(sizeof(Packet)==32);
    auto *packet=static_cast<Packet *>(argument);
    if (packet->tx_length>4096 || packet->rx_max>4096) std::abort();
    if (!record(Json::array{"kernel",id,bytes(reinterpret_cast<const uint8_t *>(packet->tx),packet->tx_length),
                           static_cast<int>(packet->rx_max),static_cast<double>(packet->timeout),packet->endpoint,packet->disconnect})) return -1;
    const size_t requested=state->input["kernel_result"].is_null() ? state->input["response"].array_items().size() : state->input["kernel_result"].int_value();
    const size_t count=std::min(requested,static_cast<size_t>(packet->rx_max));
    response(reinterpret_cast<uint8_t *>(packet->rx),count);
    return count;
  }
  std::abort();
}
}
struct Setup {
  Setup() {
    const char *input=std::getenv("PANDA_SPI_CASE");
    if (!input) return;
    auto *created=new State;
    std::string error;
    created->input=Json::parse(input,error);
    if (!error.empty()) std::abort();
    if (!created->input["initial_speed"].is_null()) created->speed=created->input["initial_speed"].int_value();
    state=created;
  }
  ~Setup() {
    State *saved=state; state=nullptr;
    if (!saved) return;
    if (const char *path=std::getenv("PANDA_SPI_TRACE")) {
      std::ofstream output(path);
      output<<Json(Json::object{{"calls",saved->calls},{"active_at_exit",static_cast<int>(saved->handles.size())}}).dump()<<'\n';
    }
    for (const auto &entry:saved->handles) original<int(*)(int)>("close")(entry.first);
    delete saved;
  }
};
static Setup setup;
