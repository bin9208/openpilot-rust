#pragma once
#include <memory>
#include "rust/cxx.h"
#include "rednose/helpers/ekf_sym.h"
#include "scheduler.h"

namespace locationd {
struct Snapshot;
struct Estimate;
class Filter {
public:
  explicit Filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise);
  Snapshot snapshot() const;
  void reset(rust::Slice<const double> x, rust::Slice<const double> covariance, double time);
  Estimate observe(double time, int kind, rust::Slice<const double> values, rust::Slice<const double> noise);
private:
  std::unique_ptr<EKFS::EKFSym> filter;
};
std::unique_ptr<Filter> new_filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise);
const EKF* pose_model();
}
