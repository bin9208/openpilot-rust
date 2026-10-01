#include "native/address.h"
#include "native/address_io.h"

namespace openpilot::cweb {
rust::String interface_ipv4(rust::Slice<const std::uint8_t> name) {
  return rust::String(interface_address(name.data(), name.size()));
}
}
