#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
namespace openpilot::cweb {
std::string interface_address(const std::uint8_t *name, std::size_t length);
}
