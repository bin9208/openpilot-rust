#pragma once
#include <cstdint>
#include <memory>
#include "rust/cxx.h"
namespace openpilot_usbgpu {
struct UsbDescription;
struct UsbTransfer;
struct UsbBulkResult;
enum class UsbSetup : std::uint8_t;
class NativeUsb {
public:
  struct Impl;
  explicit NativeUsb(std::unique_ptr<Impl> impl);
  ~NativeUsb();
  UsbDescription describe() const;
  std::int32_t setup(UsbSetup operation, std::int32_t value, std::int32_t other);
  std::int32_t streams(rust::Slice<const std::uint8_t> endpoints, std::uint32_t count);
  std::int32_t control(std::uint8_t kind, std::uint8_t request, std::uint16_t value, std::uint16_t index,
                       rust::Slice<std::uint8_t> bytes, std::uint32_t timeout);
  UsbBulkResult bulk(std::uint8_t endpoint, rust::Slice<std::uint8_t> bytes, std::uint32_t timeout);
  void batch(rust::Slice<UsbTransfer> transfers);
  rust::String error_text(std::int32_t code) const;
private:
  std::unique_ptr<Impl> impl_;
};
std::unique_ptr<NativeUsb> open_usb(std::uint16_t vendor, std::uint16_t product, std::uint32_t index);
}
