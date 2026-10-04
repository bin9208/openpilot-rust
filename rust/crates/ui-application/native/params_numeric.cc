#include "params_numeric.h"
#include "openpilot-ui-application/src/params/numeric/bridge.rs.h"
#include <stdexcept>
#include <string>
namespace product_ui {
FloatResult params_float(rust::Slice<const uint8_t> bytes) noexcept {
  if (bytes.empty()) return {0.0f, true};
  try {
    const std::string value(reinterpret_cast<const char *>(bytes.data()), bytes.size());
    return {std::stof(value), true};
  } catch (const std::invalid_argument &) {
    return {0.0f, false};
  } catch (const std::out_of_range &) {
    return {0.0f, false};
  }
}
}
