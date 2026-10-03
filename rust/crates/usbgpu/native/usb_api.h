#pragma once
#include "usb.h"
#include "vendor/libusb.h"
#include <dlfcn.h>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <vector>
namespace openpilot_usbgpu {
struct LibraryClose {
  void operator()(void *handle) const noexcept {
    if (handle && dlclose(handle)) std::fputs("usbgpu: libusb library close failed\n",stderr);
  }
};
struct UsbApi {
  std::unique_ptr<void,LibraryClose> library;
#define USB_API(name) decltype(&libusb_##name) name = nullptr;
  USB_API(init) USB_API(exit) USB_API(get_device_list) USB_API(free_device_list)
  USB_API(get_device_descriptor) USB_API(get_bus_number) USB_API(get_device_address)
  USB_API(open) USB_API(close) USB_API(get_device) USB_API(get_string_descriptor_ascii)
  USB_API(kernel_driver_active) USB_API(detach_kernel_driver) USB_API(reset_device)
  USB_API(set_configuration) USB_API(claim_interface) USB_API(release_interface)
  USB_API(set_interface_alt_setting) USB_API(clear_halt) USB_API(alloc_streams) USB_API(free_streams)
  USB_API(control_transfer) USB_API(bulk_transfer) USB_API(strerror)
  USB_API(alloc_transfer) USB_API(free_transfer) USB_API(submit_transfer) USB_API(cancel_transfer)
  USB_API(transfer_set_stream_id) USB_API(handle_events_timeout_completed)
#undef USB_API
  UsbApi();
  void checked(int code, const char *operation) const {
    if (code < 0) throw std::runtime_error(std::string(operation)+": "+strerror(code));
  }
};
struct ContextClose {
  std::shared_ptr<UsbApi> api;
  void operator()(libusb_context *context) const noexcept { if(context) api->exit(context); }
};
struct HandleClose {
  std::shared_ptr<UsbApi> api;
  void operator()(libusb_device_handle *handle) const noexcept { if(handle) api->close(handle); }
};
struct NativeUsb::Impl {
  std::shared_ptr<UsbApi> api;
  std::unique_ptr<libusb_context,ContextClose> context;
  std::unique_ptr<libusb_device_handle,HandleClose> handle;
  bool claimed=false;
  std::vector<std::uint8_t> stream_endpoints;
  Impl();
  ~Impl();
};
}
