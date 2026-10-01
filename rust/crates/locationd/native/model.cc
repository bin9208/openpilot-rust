#include "filter.h"
#include "openpilot-locationd/src/bridge.rs.h"
#include <cstring>

namespace {
#define DIM 18
#define EDIM 18
#define MEDIM 18
using Hfun = void (*)(double*, double*, double*);
void f_fun(double* x, double dt, double* out) { locationd::model_transition({x, 18}, dt, {out, 18}); }
void F_fun(double* x, double dt, double* out) { locationd::model_transition_jacobian({x, 18}, dt, {out, 324}); }
void H_mod_fun(double*, double* out) { locationd::model_identity({out, 324}); }
void err_fun(double* x, double* delta, double* out) { locationd::model_error({x, 18}, {delta, 18}, {out, 18}); }
template<int Kind> void h(double* x, double*, double* out) { locationd::model_observe(Kind, {x, 18}, {out, 3}); }
template<int Kind> void H(double* x, double*, double* out) { locationd::model_observe_jacobian(Kind, {x, 18}, {out, 54}); }
#include "rednose/templates/ekf_c.c"
template<int Kind> void update_kind(double* x, double* p, double* z, double* r, double* ea) {
  update<3, 3, false>(x, p, h<Kind>, H<Kind>, nullptr, z, r, ea, 7.814727903251177);
}
}
namespace locationd {
const EKF* pose_model() {
  static const EKF model = [] {
    EKF value;
    value.name = "pose_rust";
    value.kinds = {4, 10, 13, 14};
    value.f_fun = f_fun;
    value.F_fun = F_fun;
    value.err_fun = err_fun;
    value.inv_err_fun = nullptr;
    value.H_mod_fun = H_mod_fun;
    value.predict = predict;
    value.hs = {{4, h<4>}, {10, h<10>}, {13, h<13>}, {14, h<14>}};
    value.Hs = {{4, H<4>}, {10, H<10>}, {13, H<13>}, {14, H<14>}};
    value.updates = {{4, update_kind<4>}, {10, update_kind<10>}, {13, update_kind<13>}, {14, update_kind<14>}};
    return value;
  }();
  return &model;
}
}
