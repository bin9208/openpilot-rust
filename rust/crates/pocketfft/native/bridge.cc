#include "bridge.h"
#include "openpilot-pocketfft/src/bridge.rs.h"
#define POCKETFFT_NO_MULTITHREADING
#include "vendor/pocketfft_hdronly.h"
#include <stdexcept>
#include <vector>
namespace openpilot_pocketfft {
struct Plan::Impl {
  std::size_t size;
  std::shared_ptr<pocketfft::detail::pocketfft_c<double>> kernel;
  std::vector<pocketfft::detail::cmplx<double>> values;
  explicit Impl(std::size_t n): size(n), kernel(pocketfft::detail::get_plan<pocketfft::detail::pocketfft_c<double>>(n)), values(n) {}
};
Plan::Plan(std::size_t size) {
  if (size == 0) throw std::invalid_argument("PocketFFT length must be positive");
  impl = std::make_unique<Impl>(size);
}
Plan::~Plan() = default;
void Plan::transform(rust::Slice<Complex> data, double scale, bool forward) {
  if (data.size() != impl->size) throw std::invalid_argument("PocketFFT slice length does not match plan");
  for (std::size_t i=0; i<data.size(); ++i) impl->values[i] = {data[i].re, data[i].im};
  impl->kernel->exec(impl->values.data(), scale, forward);
  for (std::size_t i=0; i<data.size(); ++i) { data[i].re = impl->values[i].r; data[i].im = impl->values[i].i; }
}
std::unique_ptr<Plan> plan(std::size_t size) { return std::make_unique<Plan>(size); }
}
