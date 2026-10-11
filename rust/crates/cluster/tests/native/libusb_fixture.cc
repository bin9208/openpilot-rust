#define libusb_get_device_descriptor old_device_descriptor
#define libusb_set_configuration old_set_configuration
#define libusb_open old_open
#define libusb_exit old_exit
#include "../../../usbgpu/tests/native/libusb_fixture.cc"
#undef libusb_get_device_descriptor
#undef libusb_set_configuration
#undef libusb_open
#undef libusb_exit

static int references = 0;
static int configurations = 0;
struct OwnedConfiguration {
  libusb_config_descriptor config;
  libusb_endpoint_descriptor endpoints[3];
  libusb_interface_descriptor alternates[2];
  libusb_interface interfaces[1];
};
static_assert(offsetof(OwnedConfiguration, config) == 0);

static_assert(sizeof(libusb_endpoint_descriptor) == 32);
static_assert(offsetof(libusb_endpoint_descriptor, extra) == 16);
static_assert(sizeof(libusb_interface_descriptor) == 40);
static_assert(offsetof(libusb_interface_descriptor, endpoint) == 16);
static_assert(sizeof(libusb_interface) == 16);
static_assert(offsetof(libusb_interface, num_altsetting) == 8);
static_assert(sizeof(libusb_config_descriptor) == 40);
static_assert(offsetof(libusb_config_descriptor, interface) == 16);

extern "C" {
void libusb_set_debug(libusb_context *, int) {}
libusb_device *libusb_get_parent(libusb_device *) { return nullptr; }
libusb_device *libusb_ref_device(libusb_device *dev) { ++references; return dev; }
void libusb_unref_device(libusb_device *) { assert(references > 0); --references; }
int libusb_attach_kernel_driver(libusb_device_handle *, int) { return 0; }
int libusb_get_max_iso_packet_size(libusb_device *, unsigned char) { return 0; }
int libusb_interrupt_transfer(libusb_device_handle *, unsigned char, unsigned char *, int, int *, unsigned int) { assert(false); return -1; }
int libusb_handle_events(libusb_context *) { assert(false); return -1; }
void libusb_exit(libusb_context *ctx) {
  assert(references == 0 && configurations == 0);
  old_exit(ctx);
}
int libusb_get_device_descriptor(libusb_device *, libusb_device_descriptor *out) {
  old_device_descriptor(&device, out);
  out->bLength = 18;
  out->bDescriptorType = 1;
  out->idVendor = 0x1cbe;
  out->idProduct = mode("pid123") ? 0x123 : mode("missing") ? 0xff : 0x92;
  out->bNumConfigurations = 1;
  return 0;
}
int libusb_open(libusb_device *dev, libusb_device_handle **out) {
  record("open requested");
  return old_open(dev, out);
}
int libusb_set_configuration(libusb_device_handle *dev, int value) {
  assert(value == 7);
  record("configured 7");
  return old_set_configuration(dev, value);
}
int libusb_get_configuration(libusb_device_handle *, int *value) { *value = 7; return 0; }
int libusb_get_config_descriptor(libusb_device *, uint8_t index, libusb_config_descriptor **out) {
  assert(index == 0);
  record("configuration queried");
  if (mode("config_error")) return LIBUSB_ERROR_IO;
  if (mode("null_config")) { *out = nullptr; return 0; }
  auto *owned = new OwnedConfiguration{};
  auto *config = &owned->config;
  auto *endpoints = owned->endpoints;
  auto *alternates = owned->alternates;
  auto *interfaces = owned->interfaces;
  ++configurations;
  config->bConfigurationValue = 7;
  config->bNumInterfaces = 1;
  config->interface = mode("null_interfaces") ? nullptr : interfaces;
  endpoints[0] = {}; endpoints[0].bEndpointAddress = 0x84; endpoints[0].bmAttributes = 3; endpoints[0].wMaxPacketSize = 64;
  endpoints[1] = {}; endpoints[1].bEndpointAddress = 7; endpoints[1].bmAttributes = 1; endpoints[1].wMaxPacketSize = 512;
  endpoints[2] = {}; endpoints[2].bEndpointAddress = 0x81; endpoints[2].bmAttributes = 2; endpoints[2].wMaxPacketSize = 1024;
  alternates[0] = {}; alternates[0].bInterfaceNumber = mode("no_interface0") ? 1 : 0; alternates[0].bAlternateSetting = 2;
  alternates[0].bNumEndpoints = 3; alternates[0].endpoint = mode("null_endpoints") ? nullptr : endpoints;
  alternates[1] = {}; alternates[1].bInterfaceNumber = 0;
  interfaces[0] = {}; interfaces[0].altsetting = mode("null_alternates") ? nullptr : alternates;
  interfaces[0].num_altsetting = mode("negative_alternates") ? -1 : 2;
  *out = config;
  return mode("config_error_owned") ? LIBUSB_ERROR_IO : 0;
}
void libusb_free_config_descriptor(libusb_config_descriptor *config) {
  assert(config && configurations > 0);
  --configurations;
  delete reinterpret_cast<OwnedConfiguration *>(config);
  record("configuration freed");
}
}
