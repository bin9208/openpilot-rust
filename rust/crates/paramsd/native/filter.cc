#include "filter.h"
#include "openpilot-paramsd/src/bridge.rs.h"
#include <mutex>
#include <stdexcept>

namespace paramsd {
using EKFS::MatrixXdr;
namespace {
void size(rust::Slice<const double> values, size_t expected) {
  if (values.size() != expected) throw std::invalid_argument("invalid CarKalman dimensions");
}
rust::Vec<double> copy(const double* values, size_t length) {
  rust::Vec<double> out;
  out.reserve(length);
  for (size_t i = 0; i < length; ++i) out.push_back(values[i]);
  return out;
}
}
Filter::Filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise, rust::Slice<const double> globals) {
  size(x, 9); size(covariance, 81); size(noise, 81); size(globals, 6);
  std::copy(globals.begin(), globals.end(), this->globals.begin());
  static std::once_flag registered;
  std::call_once(registered, [] { ekf_register(car_model()); });
  Eigen::VectorXd state = Eigen::Map<const Eigen::VectorXd>(x.data(), 9);
  MatrixXdr p = Eigen::Map<const MatrixXdr>(covariance.data(), 9, 9);
  MatrixXdr q = Eigen::Map<const MatrixXdr>(noise.data(), 9, 9);
  filter = std::make_unique<EKFS::EKFSym>("car_rust", Eigen::Map<MatrixXdr>(q.data(), 9, 9),
    Eigen::Map<Eigen::VectorXd>(state.data(), 9), Eigen::Map<MatrixXdr>(p.data(), 9, 9),
    9, 9, 0, 0, 0, std::vector<int>{}, std::vector<int>{}, std::vector<std::string>{}, 1.0);
}
std::unique_ptr<Filter> new_filter(rust::Slice<const double> x, rust::Slice<const double> covariance, rust::Slice<const double> noise, rust::Slice<const double> globals) {
  return std::make_unique<Filter>(x, covariance, noise, globals);
}
Snapshot Filter::snapshot() const {
  auto x = filter->state();
  auto p = filter->covs();
  return Snapshot{filter->get_filter_time(), copy(x.data(), 9), copy(p.data(), 81)};
}
void Filter::pause(double time) { filter->set_filter_time(time); filter->reset_rewind(); }
void Filter::reset(rust::Slice<const double> x, rust::Slice<const double> covariance, double time) {
  size(x, 9); size(covariance, 81);
  Eigen::VectorXd state = Eigen::Map<const Eigen::VectorXd>(x.data(), 9);
  MatrixXdr p = Eigen::Map<const MatrixXdr>(covariance.data(), 9, 9);
  filter->init_state(Eigen::Map<Eigen::VectorXd>(state.data(), 9), Eigen::Map<MatrixXdr>(p.data(), 9, 9), time);
}
Estimate Filter::observe(double time, int kind, rust::Slice<const double> values, rust::Slice<const double> noise) {
  const int dim = kind == 24 ? 2 : 1;
  size(values, dim); size(noise, dim * dim);
  if (kind < 24 || kind > 31) throw std::invalid_argument("invalid observation kind");
  if (!std::isfinite(time)) throw std::invalid_argument("nonfinite observation time");
  Eigen::VectorXd z = Eigen::Map<const Eigen::VectorXd>(values.data(), dim);
  MatrixXdr r = Eigen::Map<const MatrixXdr>(noise.data(), dim, dim);
  struct Scope {
    const double* previous;
    explicit Scope(const double* values) : previous(current_globals) { current_globals = values; }
    ~Scope() { current_globals = previous; }
  } scope(globals.data());
  auto result = filter->predict_and_update_batch(time, kind,
    {Eigen::Map<Eigen::VectorXd>(z.data(), dim)}, {Eigen::Map<MatrixXdr>(r.data(), dim, dim)});
  if (!result) return Estimate{false, {}, {}, {}};
  return Estimate{true, copy(result->xk.data(), 9), copy(result->Pk.data(), 81), copy(result->y.at(0).data(), dim)};
}
}
