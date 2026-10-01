#include "vendor/libusb.h"
#include <cassert>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
struct libusb_context { int value; };
struct libusb_device { int value; };
struct libusb_device_handle { int value; };
static libusb_device device{1};
static int live_contexts=0, live_handles=0, live_transfers=0, submissions=0, events=0;
static std::vector<libusb_transfer*> pending;
static bool mode(const char *value) { const char *m=std::getenv("USB_FIXTURE_MODE"); return m && !std::strcmp(m,value); }
static void record(const char *message) { if(const char *path=std::getenv("USB_FIXTURE_LOG")) {FILE *f=std::fopen(path,"a");assert(f);std::fprintf(f,"%s\n",message);std::fclose(f);} }
extern "C" {
int libusb_init(libusb_context **out) { *out=new libusb_context{1};++live_contexts;record("init");return 0; }
void libusb_exit(libusb_context *context) {assert(live_handles==0 && live_transfers==0 && pending.empty());delete context;--live_contexts;record("exit clean");}
ssize_t libusb_get_device_list(libusb_context *,libusb_device ***out) { *out=new libusb_device*[2]{&device,nullptr};return 1; }
void libusb_free_device_list(libusb_device **list,int) {delete[] list;record("list freed");}
int libusb_get_device_descriptor(libusb_device *,libusb_device_descriptor *out) {std::memset(out,0,sizeof(*out));out->idVendor=0xadd1;out->idProduct=0x0001;out->iProduct=1;return 0;}
uint8_t libusb_get_bus_number(libusb_device *) {return 7;}
uint8_t libusb_get_device_address(libusb_device *) {return 9;}
int libusb_open(libusb_device *,libusb_device_handle **out) {if(mode("open_error"))return LIBUSB_ERROR_ACCESS;*out=new libusb_device_handle{1};++live_handles;return 0;}
void libusb_close(libusb_device_handle *handle) {assert(pending.empty() && live_transfers==0);delete handle;--live_handles;record("handle closed");}
libusb_device *libusb_get_device(libusb_device_handle *) {return &device;}
int libusb_get_string_descriptor_ascii(libusb_device_handle *,uint8_t,unsigned char *out,int length) {assert(length>=4);std::memcpy(out,"test",4);return 4;}
int libusb_kernel_driver_active(libusb_device_handle *,int) {return 1;}
int libusb_detach_kernel_driver(libusb_device_handle *,int) {return 0;}
int libusb_reset_device(libusb_device_handle *) {return 0;}
int libusb_set_configuration(libusb_device_handle *,int) {return 0;}
int libusb_claim_interface(libusb_device_handle *,int value) {assert(value==0);record("claimed");return 0;}
int libusb_release_interface(libusb_device_handle *,int value) {assert(value==0);record("released");return 0;}
int libusb_set_interface_alt_setting(libusb_device_handle *,int,int) {return 0;}
int libusb_clear_halt(libusb_device_handle *,unsigned char) {return 0;}
int libusb_alloc_streams(libusb_device_handle *,uint32_t count,unsigned char *,int) {record("streams allocated");return count;}
int libusb_free_streams(libusb_device_handle *,unsigned char *,int) {record("streams freed");return 0;}
int libusb_control_transfer(libusb_device_handle *,uint8_t,uint8_t,uint16_t,uint16_t,unsigned char *out,uint16_t length,unsigned int) {if(mode("control_error"))return LIBUSB_ERROR_IO;std::memset(out,0xa5,length);return length;}
int libusb_bulk_transfer(libusb_device_handle *,unsigned char,unsigned char *out,int length,int *actual,unsigned int) {std::memset(out,0x5a,length);*actual=mode("partial")?length/2:length;return mode("partial")?LIBUSB_ERROR_IO:0;}
const char *libusb_strerror(int code) {return code<0?"fixture USB failure":"ok";}
libusb_transfer *libusb_alloc_transfer(int) {++live_transfers;return static_cast<libusb_transfer*>(std::calloc(1,sizeof(libusb_transfer)));}
void libusb_free_transfer(libusb_transfer *transfer) {for(auto *p:pending)assert(p!=transfer);std::free(transfer);--live_transfers;record("transfer freed");}
int libusb_submit_transfer(libusb_transfer *transfer) {if(mode("submit_error") && ++submissions==2)return LIBUSB_ERROR_IO;pending.push_back(transfer);record("submitted");return 0;}
int libusb_cancel_transfer(libusb_transfer *transfer) {transfer->status=LIBUSB_TRANSFER_CANCELLED;record("cancelled");return 0;}
void libusb_transfer_set_stream_id(libusb_transfer *,uint32_t) {}
int libusb_handle_events_timeout_completed(libusb_context *,timeval *,int *) {++events;if(events==1 && mode("event_error"))return LIBUSB_ERROR_IO;if(events==1 && mode("interrupted"))return LIBUSB_ERROR_INTERRUPTED;if(!pending.empty()) {auto *transfer=pending.back();pending.pop_back();if(transfer->status!=LIBUSB_TRANSFER_CANCELLED) {transfer->status=LIBUSB_TRANSFER_COMPLETED;transfer->actual_length=transfer->length;std::memset(transfer->buffer,0x3c,transfer->length);}record("callback");transfer->callback(transfer);}return 0;}
}
