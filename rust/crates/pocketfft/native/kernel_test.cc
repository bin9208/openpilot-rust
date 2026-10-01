#define POCKETFFT_NO_MULTITHREADING
#include "vendor/pocketfft_hdronly.h"
#include <cassert>
#include <cmath>
#include <iostream>
#include <vector>
int main() {
  for (std::size_t size : {1,2,7,11,40,200,1225}) {
    auto plan=pocketfft::detail::get_plan<pocketfft::detail::pocketfft_c<double>>(size);
    std::vector<pocketfft::detail::cmplx<double>> values(size);
    for (auto &value:values) value={0.,0.};
    values[0].r=1.;
    for (int repetition=0;repetition<32;++repetition) {
      plan->exec(values.data(),1.,true);
      for (auto value:values) assert(std::abs(value.r-1.)<1e-12 && std::abs(value.i)<1e-12);
      plan->exec(values.data(),1./double(size),false);
      assert(std::abs(values[0].r-1.)<1e-12);
      for (std::size_t i=1;i<size;++i) assert(std::abs(values[i].r)<1e-12 && std::abs(values[i].i)<1e-12);
    }
  }
  std::cout << "PASS: 224 owned PocketFFT forward/inverse round trips under ASan/UBSan\n";
}
