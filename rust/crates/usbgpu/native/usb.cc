#include "usb_api.h"
#include "openpilot-usbgpu/src/usb_bridge.rs.h"
#include <limits>
namespace openpilot_usbgpu {
UsbApi::UsbApi() : library(dlopen("libusb-1.0.so.0",RTLD_NOW|RTLD_LOCAL)) {
  if (!library) throw std::runtime_error(dlerror());
#define USB_LOAD(name) \
  name=reinterpret_cast<decltype(name)>(dlsym(library.get(),"libusb_" #name)); \
  if (!name) throw std::runtime_error("libusb symbol missing: " #name);
  USB_LOAD(init) USB_LOAD(exit) USB_LOAD(get_device_list) USB_LOAD(free_device_list)
  USB_LOAD(get_device_descriptor) USB_LOAD(get_bus_number) USB_LOAD(get_device_address)
  USB_LOAD(open) USB_LOAD(close) USB_LOAD(get_device) USB_LOAD(get_string_descriptor_ascii)
  USB_LOAD(kernel_driver_active) USB_LOAD(detach_kernel_driver) USB_LOAD(reset_device)
  USB_LOAD(set_configuration) USB_LOAD(claim_interface) USB_LOAD(release_interface)
  USB_LOAD(set_interface_alt_setting) USB_LOAD(clear_halt) USB_LOAD(alloc_streams) USB_LOAD(free_streams)
  USB_LOAD(control_transfer) USB_LOAD(bulk_transfer) USB_LOAD(strerror)
  USB_LOAD(alloc_transfer) USB_LOAD(free_transfer) USB_LOAD(submit_transfer) USB_LOAD(cancel_transfer)
  USB_LOAD(transfer_set_stream_id) USB_LOAD(handle_events_timeout_completed)
#undef USB_LOAD
}
NativeUsb::Impl::Impl() : api(std::make_shared<UsbApi>()),context(nullptr,{api}),handle(nullptr,{api}) {
  libusb_context *raw=nullptr;
  const int status=api->init(&raw);
  context.reset(raw);
  api->checked(status,"libusb_init");
  if (!context) throw std::runtime_error("libusb_init returned null context");
}
NativeUsb::Impl::~Impl() {
  if (!handle) return;
  if (!stream_endpoints.empty() && api->free_streams(handle.get(),stream_endpoints.data(),stream_endpoints.size())<0)
    std::fputs("usbgpu: release USB streams failed\n",stderr);
  if (claimed && api->release_interface(handle.get(),0)<0) std::fputs("usbgpu: release USB interface failed\n",stderr);
}
NativeUsb::NativeUsb(std::unique_ptr<Impl> impl) : impl_(std::move(impl)) {}
NativeUsb::~NativeUsb()=default;
std::unique_ptr<NativeUsb> open_usb(std::uint16_t vendor,std::uint16_t product,std::uint32_t index) {
  auto state=std::make_unique<NativeUsb::Impl>();
  libusb_device **raw=nullptr;
  const auto count=state->api->get_device_list(state->context.get(),&raw);
  const auto release=[api=state->api](libusb_device **list) { if(list)api->free_device_list(list,1); };
  std::unique_ptr<libusb_device *,decltype(release)> devices(raw,release);
  state->api->checked(count<0?static_cast<int>(count):0,"libusb_get_device_list");
  if (count>0 && !raw) throw std::runtime_error("libusb returned null device list");
  for (ssize_t i=0;i<count;++i) {
    libusb_device_descriptor description{};
    state->api->checked(state->api->get_device_descriptor(raw[i],&description),"libusb_get_device_descriptor");
    if (description.idVendor!=vendor || description.idProduct!=product) continue;
    if (index) {--index;continue;}
    libusb_device_handle *handle=nullptr;
    const int status=state->api->open(raw[i],&handle);
    state->handle.reset(handle);
    state->api->checked(status,"libusb_open");
    if (!handle) throw std::runtime_error("libusb_open returned null handle");
    return std::make_unique<NativeUsb>(std::move(state));
  }
  return nullptr;
}
UsbDescription NativeUsb::describe() const {
  auto *device=impl_->api->get_device(impl_->handle.get());
  libusb_device_descriptor descriptor{};
  impl_->api->checked(impl_->api->get_device_descriptor(device,&descriptor),"libusb_get_device_descriptor");
  std::uint8_t bytes[256]{};
  const auto length=impl_->api->get_string_descriptor_ascii(impl_->handle.get(),descriptor.iProduct,bytes,sizeof(bytes));
  impl_->api->checked(length,"libusb_get_string_descriptor_ascii");
  if (length>256) throw std::runtime_error("libusb product descriptor exceeds buffer");
  rust::Vec<std::uint8_t> product;
  for(int i=0;i<length;++i) product.push_back(bytes[i]);
  return {impl_->api->get_bus_number(device),impl_->api->get_device_address(device),std::move(product)};
}
std::int32_t NativeUsb::setup(UsbSetup operation,std::int32_t value,std::int32_t other) {
  auto &a=*impl_->api;
  auto *h=impl_->handle.get();
  switch(operation) {
    case UsbSetup::KernelActive:return a.kernel_driver_active(h,value);
    case UsbSetup::Detach:return a.detach_kernel_driver(h,value);
    case UsbSetup::Reset:return a.reset_device(h);
    case UsbSetup::Configuration:return a.set_configuration(h,value);
    case UsbSetup::Claim:{const int status=a.claim_interface(h,value);if(status>=0)impl_->claimed=true;return status;}
    case UsbSetup::Alternate:return a.set_interface_alt_setting(h,value,other);
    case UsbSetup::ClearHalt:
      if(value<0 || value>255)throw std::invalid_argument("invalid USB endpoint");
      return a.clear_halt(h,static_cast<unsigned char>(value));
    default:throw std::invalid_argument("invalid USB setup operation");
  }
}
std::int32_t NativeUsb::streams(rust::Slice<const std::uint8_t> endpoints,std::uint32_t count) {
  if(endpoints.size()>32)throw std::invalid_argument("USB stream endpoint count");
  std::vector<std::uint8_t> copy(endpoints.begin(),endpoints.end());
  const int status=impl_->api->alloc_streams(impl_->handle.get(),count,copy.data(),static_cast<int>(copy.size()));
  if(status>=0)impl_->stream_endpoints=std::move(copy);
  return status;
}
std::int32_t NativeUsb::control(std::uint8_t kind,std::uint8_t request,std::uint16_t value,std::uint16_t index,
                              rust::Slice<std::uint8_t> bytes,std::uint32_t timeout) {
  if(bytes.size()>std::numeric_limits<std::uint16_t>::max())throw std::invalid_argument("USB control buffer size");
  const int result=impl_->api->control_transfer(impl_->handle.get(),kind,request,value,index,bytes.data(),static_cast<std::uint16_t>(bytes.size()),timeout);
  if(result>=0 && static_cast<std::size_t>(result)>bytes.size())throw std::runtime_error("USB control length exceeds buffer");
  return result;
}
UsbBulkResult NativeUsb::bulk(std::uint8_t endpoint,rust::Slice<std::uint8_t> bytes,std::uint32_t timeout) {
  if(bytes.size()>std::numeric_limits<int>::max())throw std::invalid_argument("USB bulk buffer size");
  int actual=0;
  const int code=impl_->api->bulk_transfer(impl_->handle.get(),endpoint,bytes.data(),static_cast<int>(bytes.size()),&actual,timeout);
  if(actual<0 || static_cast<std::size_t>(actual)>bytes.size())throw std::runtime_error("USB bulk length outside buffer");
  return {code,static_cast<std::uint32_t>(actual)};
}
rust::String NativeUsb::error_text(std::int32_t code) const { return impl_->api->strerror(code); }
}
