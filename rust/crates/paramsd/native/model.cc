#include "filter.h"
#include "openpilot-paramsd/src/bridge.rs.h"
#include <cstring>

namespace paramsd { thread_local const double* current_globals = nullptr; }
namespace {
#define DIM 9
#define EDIM 9
#define MEDIM 9
using Hfun = void (*)(double*, double*, double*);
void f_fun(double* x, double dt, double* out) { paramsd::model_transition({x, 9}, {paramsd::current_globals, 6}, dt, {out, 9}); }
void F_fun(double* x, double dt, double* out) { paramsd::model_transition_jacobian({x, 9}, {paramsd::current_globals, 6}, dt, {out, 81}); }
void H_mod_fun(double*, double* out) { paramsd::model_identity({out, 81}); }
void err_fun(double* x, double* delta, double* out) { paramsd::model_error({x, 9}, {delta, 9}, {out, 9}); }
template<int Kind> void h(double* x, double*, double* out) { paramsd::model_observe(Kind, {x, 9}, {out, Kind == 24 ? 2u : 1u}); }
template<int Kind> void H(double* x, double*, double* out) { paramsd::model_observe_jacobian(Kind, {x, 9}, {out, Kind == 24 ? 18u : 9u}); }
#include "rednose/templates/ekf_c.c"
template<int Kind> void update_kind(double* x, double* p, double* z, double* r, double* ea) {
  constexpr int dim = Kind == 24 ? 2 : 1;
  update<dim, dim, false>(x, p, h<Kind>, H<Kind>, nullptr, z, r, ea, 3.841458820694124);
}
}
namespace paramsd {
const EKF* car_model() {
  static const EKF model = [] {
    EKF value;
    value.name = "car_rust";
    value.kinds = {24, 25, 26, 27, 28, 29, 30, 31};
    value.f_fun = f_fun;
    value.F_fun = F_fun;
    value.err_fun = err_fun;
    value.inv_err_fun = nullptr;
    value.H_mod_fun = H_mod_fun;
    value.predict = predict;
    value.hs = {{24, h<24>}, {25, h<25>}, {26, h<26>}, {27, h<27>}, {28, h<28>}, {29, h<29>}, {30, h<30>}, {31, h<31>}};
    value.Hs = {{24, H<24>}, {25, H<25>}, {26, H<26>}, {27, H<27>}, {28, H<28>}, {29, H<29>}, {30, H<30>}, {31, H<31>}};
    value.updates = {{24, update_kind<24>}, {25, update_kind<25>}, {26, update_kind<26>}, {27, update_kind<27>}, {28, update_kind<28>}, {29, update_kind<29>}, {30, update_kind<30>}, {31, update_kind<31>}};
    return value;
  }();
  return &model;
}
}
