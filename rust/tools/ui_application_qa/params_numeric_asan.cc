#include "params_numeric.h"
#include "openpilot-ui-application/src/params/numeric/bridge.rs.h"
#include <cmath>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
static product_ui::FloatResult parse(const std::string &bytes) {
  return product_ui::params_float({reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size()});
}
int main() {
  for (int iteration = 0; iteration < 1000; ++iteration) {
    for (const auto &[input, expected] : std::vector<std::pair<std::string, float>>{
          {"", 0}, {" \t+0.1tail", 0.1f}, {std::string("12.5\0-99", 8), 12.5f},
          {"0x1.8p+2tail", 6}, {"16777217", 16777216}, {"3.4028234e38", 3.402823466e38f}}) {
      const auto result = parse(input);
      if (!result.valid || result.value != expected) throw std::runtime_error("float prefix mismatch");
    }
    for (const auto &input : {std::string("invalid"), std::string("\0 7", 3), std::string("1e39"), std::string("1e-50")}) {
      if (parse(input).valid) throw std::runtime_error("invalid/range value accepted");
    }
    if (!std::isnan(parse("nan(payload)").value) || !std::signbit(parse("-0suffix").value)) throw std::runtime_error("nonfinite/signed zero mismatch");
  }
  const auto result = parse(std::string("2.5") + std::string(1024 * 1024, 'x'));
  if (!result.valid || result.value != 2.5f) throw std::runtime_error("large borrowed input mismatch");
  std::cout << "PASS 1000 borrowed parser boundary cycles and one 1 MiB prefix input\n";
}
