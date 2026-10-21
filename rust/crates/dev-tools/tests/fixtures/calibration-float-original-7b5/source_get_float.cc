#include <string>
#include <iostream>
#include <iterator>
#include <iomanip>
struct Source {
  std::string value;
  std::string get(const std::string&, bool) { return value; }
  inline float getFloat(const std::string &key, bool block = false) {
    std::string value = get(key, block);
    return value.empty() ? 0.0 : std::stof(value);
  }
};
int main() {
  Source params;
  params.value = std::string(std::istreambuf_iterator<char>(std::cin), {});
  try { std::cout << std::setprecision(17) << double(params.getFloat("CameraYawTrimDeg")); }
  catch(const std::exception&) { std::cout << "ERROR"; }
}
