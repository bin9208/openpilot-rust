#define libusb_bulk_transfer old_bulk_transfer
#define libusb_clear_halt old_clear_halt
#define libusb_reset_device old_reset_device
#define libusb_get_config_descriptor metadata_get_config_descriptor
#include "libusb_fixture.cc"
#undef libusb_bulk_transfer
#undef libusb_clear_halt
#undef libusb_reset_device
#undef libusb_get_config_descriptor

static bool waiting_ack = false;
static int writes = 0;
static int ack_reads = 0;
static int flow_reads = 0;

static void wire(const char *operation, int endpoint, int length, int actual, unsigned timeout, int code) {
  if(const char *path=std::getenv("USB_FIXTURE_WIRE")) {
    FILE *f=std::fopen(path,"a");assert(f);
    std::fprintf(f,"%s %d %d %d %u %d\n",operation,endpoint,length,actual,timeout,code);
    std::fclose(f);
  }
}

extern "C" {
int libusb_get_config_descriptor(libusb_device *device, uint8_t index, libusb_config_descriptor **out) {
  int code=metadata_get_config_descriptor(device,index,out);
  if(code || !*out)return code;
  auto *owned=reinterpret_cast<OwnedConfiguration *>(*out);
  owned->alternates[0].bAlternateSetting=0;
  owned->endpoints[0].bmAttributes=2;
  owned->endpoints[1].bmAttributes=2;
  return code;
}
int libusb_clear_halt(libusb_device_handle *, unsigned char endpoint) {
  int code=mode("clear_error") ? LIBUSB_ERROR_PIPE : 0;
  wire("clear", endpoint, 0, 0, 0, code); return code;
}
int libusb_reset_device(libusb_device_handle *) {
  waiting_ack=false;
  wire("reset",0,0,0,0,0);return 0;
}
int libusb_bulk_transfer(libusb_device_handle *, unsigned char endpoint, unsigned char *bytes,
                        int length, int *actual, unsigned int timeout) {
  assert(endpoint==7 || endpoint==0x84);
  if(endpoint==7) {
    ++writes;
    int code=((mode("write_error") || mode("disconnect") || mode("write_io")) && writes>=4) ?
      (mode("disconnect") ? LIBUSB_ERROR_NO_DEVICE : mode("write_io") ? LIBUSB_ERROR_IO : LIBUSB_ERROR_TIMEOUT) : 0;
    if(mode("intermittent") && (writes==4 || writes==9 || writes==14))code=LIBUSB_ERROR_TIMEOUT;
    *actual=code ? 0 : mode("short_write") ? length/2 : length;
    wire("write",endpoint,length,*actual,timeout,code);
    if(const char *path=std::getenv("USB_FIXTURE_BYTES")) {
      FILE *f=std::fopen(path,"ab");assert(f);
      std::fwrite(&length,sizeof(length),1,f);std::fwrite(bytes,1,length,f);std::fclose(f);
    }
    waiting_ack=(code==0);
    return code;
  }
  assert(length==512);
  if(timeout!=2000 || !waiting_ack) {
    if(mode("drain_success")) {
      *actual=12;std::memset(bytes,0,length);wire("read",endpoint,length,*actual,timeout,0);return 0;
    }
    *actual=0;wire("read",endpoint,length,0,timeout,LIBUSB_ERROR_TIMEOUT);
    return LIBUSB_ERROR_TIMEOUT;
  }
  waiting_ack=false;++ack_reads;
  if(mode("input_disconnect") && writes>=4) {
    *actual=0;wire("read",endpoint,length,0,timeout,LIBUSB_ERROR_NO_DEVICE);return LIBUSB_ERROR_NO_DEVICE;
  }
  if((mode("sync_timeout_once") && ack_reads==1) ||
     (mode("ack_timeout") && writes>=4)) {
    *actual=0;wire("read",endpoint,length,0,timeout,LIBUSB_ERROR_TIMEOUT);
    return LIBUSB_ERROR_TIMEOUT;
  }
  *actual=mode("empty_ack") && writes>=4 ? 0 : 12;
  std::memset(bytes,0,length);
  const unsigned chunk=mode("invalid_chunk") ? 1024*1024+1 : 65536;
  bytes[8]=static_cast<unsigned char>(chunk>>24);bytes[9]=static_cast<unsigned char>(chunk>>16);
  bytes[10]=static_cast<unsigned char>(chunk>>8);bytes[11]=static_cast<unsigned char>(chunk);
  if(mode("busy_status") && writes>=4)bytes[8]=(++flow_reads<=3) ? 5 : 2;
  wire("read",endpoint,length,*actual,timeout,0);
  return 0;
}
}
