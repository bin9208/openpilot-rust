#pragma once
#include <memory>
#include <array>
#include "rust/cxx.h"
#include "rednose/helpers/ekf_sym.h"
#include "scheduler.h"

namespace paramsd {
struct Snapshot;
struct Estimate;
class Filter {
public:
  explicit Filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise, rust::Slice<const double> globals);
  Snapshot snapshot() const;
  void reset(rust::Slice<const double> x, rust::Slice<const double> covariance, double time);
  void pause(double time);
  Estimate observe(double time, int kind, rust::Slice<const double> values, rust::Slice<const double> noise);
private:
  std::unique_ptr<EKFS::EKFSym> filter;
  std::array<double, 6> globals;
};
std::unique_ptr<Filter> new_filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise, rust::Slice<const double> globals);
const EKF* car_model();
extern thread_local const double* current_globals;
}
