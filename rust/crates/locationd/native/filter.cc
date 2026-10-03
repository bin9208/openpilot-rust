#include "filter.h"
#include "openpilot-locationd/src/bridge.rs.h"
#include <mutex>
#include <stdexcept>

namespace locationd {
using EKFS::MatrixXdr;
namespace {
void size(rust::Slice<const double> values, size_t expected) {
  if (values.size() != expected) throw std::invalid_argument("invalid PoseKalman dimensions");
}
rust::Vec<double> copy(const double* values, size_t length) {
  rust::Vec<double> out;
  out.reserve(length);
  for (size_t i = 0; i < length; ++i) out.push_back(values[i]);
  return out;
}
}
Filter::Filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise) {
  size(x, 18); size(covariance, 324); size(noise, 324);
  static std::once_flag registered;
  std::call_once(registered, [] { ekf_register(pose_model()); });
  Eigen::VectorXd state = Eigen::Map<const Eigen::VectorXd>(x.data(), 18);
  MatrixXdr p = Eigen::Map<const MatrixXdr>(covariance.data(), 18, 18);
  MatrixXdr q = Eigen::Map<const MatrixXdr>(noise.data(), 18, 18);
  filter = std::make_unique<EKFS::EKFSym>("pose_rust", Eigen::Map<MatrixXdr>(q.data(), 18, 18),
    Eigen::Map<Eigen::VectorXd>(state.data(), 18), Eigen::Map<MatrixXdr>(p.data(), 18, 18),
    18, 18, 0, 0, 0, std::vector<int>{}, std::vector<int>{}, std::vector<std::string>{}, 0.8);
}
std::unique_ptr<Filter> new_filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise) {
  return std::make_unique<Filter>(x, covariance, noise);
}
Snapshot Filter::snapshot() const {
  auto x = filter->state();
  auto p = filter->covs();
  return Snapshot{filter->get_filter_time(), copy(x.data(), 18), copy(p.data(), 324)};
}
void Filter::reset(rust::Slice<const double> x, rust::Slice<const double> covariance, double time) {
  size(x, 18); size(covariance, 324);
  Eigen::VectorXd state = Eigen::Map<const Eigen::VectorXd>(x.data(), 18);
  MatrixXdr p = Eigen::Map<const MatrixXdr>(covariance.data(), 18, 18);
  filter->init_state(Eigen::Map<Eigen::VectorXd>(state.data(), 18), Eigen::Map<MatrixXdr>(p.data(), 18, 18), time);
}
Estimate Filter::observe(double time, int kind, rust::Slice<const double> values, rust::Slice<const double> noise) {
  size(values, 3); size(noise, 9);
  if (kind != 4 && kind != 10 && kind != 13 && kind != 14) throw std::invalid_argument("invalid observation kind");
  if (!std::isfinite(time)) throw std::invalid_argument("nonfinite observation time");
  Eigen::VectorXd z = Eigen::Map<const Eigen::VectorXd>(values.data(), 3);
  MatrixXdr r = Eigen::Map<const MatrixXdr>(noise.data(), 3, 3);
  auto result = filter->predict_and_update_batch(time, kind,
    {Eigen::Map<Eigen::VectorXd>(z.data(), 3)}, {Eigen::Map<MatrixXdr>(r.data(), 3, 3)});
  if (!result) return Estimate{false, {}, {}, {}};
  return Estimate{true, copy(result->xk.data(), 18), copy(result->Pk.data(), 324), copy(result->y.at(0).data(), 3)};
}
}
