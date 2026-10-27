// Actions-only ABI calls for the exact foreign components. This is not an MPC,
// full daemon, model or device regression. Signatures match current Rust FFIs.
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>

namespace fs = std::filesystem;
const char *maps_output = nullptr;
void require(bool condition, const char *message) {
  if (!condition) throw std::runtime_error(message);
}
void record_maps() {
  std::ifstream input("/proc/self/maps");
  std::ofstream output(maps_output, std::ios::app);
  require(input.good() && output.good(), "provider maps input/output");
  output << input.rdbuf();
  require(output.good(), "provider maps write");
}
struct Library {
  void *handle;
  explicit Library(const fs::path &path) : handle(dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL)) {
    if (!handle) throw std::runtime_error(dlerror());
  }
  ~Library() { dlclose(handle); }
  Library(const Library &) = delete;
  Library &operator=(const Library &) = delete;
  template <typename T> T get(const char *name) const {
    void *symbol = dlsym(handle, name);
    require(symbol != nullptr, name);
    return reinterpret_cast<T>(symbol);
  }
};

void numerics(const fs::path &root) {
  Library dependency(root / "libgfortran-e1b7dfc8-d8198c01.so.5.0.0");
  Library api(root / "libscipy_openblas64_-f552eb69.so");
  const char *config = api.get<const char *(*)()>("scipy_openblas_get_config64_")();
  require(config && std::strstr(config, "OpenBLAS 0.3.34.106.0") && std::strstr(config, "USE64BITINT"), "NumPy/OpenBLAS version/ILP64");
  using Ddot = double (*)(int64_t, const double *, int64_t, const double *, int64_t);
  using Sdot = float (*)(int64_t, const float *, int64_t, const float *, int64_t);
  const double left[] = {1, 2, 3}, right[] = {4, 5, 6};
  const float small_left[] = {1, 2, 3}, small_right[] = {4, 5, 6};
  require(api.get<Ddot>("scipy_cblas_ddot64_")(3, left, 1, right, 1) == 32.0, "double dot");
  require(api.get<Sdot>("scipy_cblas_sdot64_")(3, small_left, 1, small_right, 1) == 32.0f, "float dot");
  using Dgemv = void (*)(int, int, int64_t, int64_t, double, const double *, int64_t, const double *, int64_t, double, double *, int64_t);
  using Sgemv = void (*)(int, int, int64_t, int64_t, float, const float *, int64_t, const float *, int64_t, float, float *, int64_t);
  const double matrix[] = {1, 2, 3, 4}, vector[] = {5, 6};
  const float small_matrix[] = {1, 2, 3, 4}, small_vector[] = {5, 6};
  double output[2] = {};
  float small_output[2] = {};
  api.get<Dgemv>("scipy_cblas_dgemv64_")(102, 112, 2, 2, 1.0, matrix, 2, vector, 1, 0.0, output, 1);
  api.get<Sgemv>("scipy_cblas_sgemv64_")(102, 112, 2, 2, 1.0f, small_matrix, 2, small_vector, 1, 0.0f, small_output, 1);
  require(output[0] == 17.0 && output[1] == 39.0 && small_output[0] == 17.0f && small_output[1] == 39.0f, "float/double matrix ABI");
  using Dgesdd = void (*)(const char *, const int64_t *, const int64_t *, double *, const int64_t *, double *, double *, const int64_t *, double *, const int64_t *, double *, const int64_t *, int64_t *, int64_t *);
  auto gesdd = api.get<Dgesdd>("scipy_dgesdd_64_");
  const char job = 'N';
  const int64_t size = 2, leading = 2, unused_leading = 1, query_length = -1;
  double diagonal[] = {3, 0, 0, 2}, singular[2] = {}, unused_u[1] = {}, unused_vt[1] = {}, query = 0;
  int64_t integer_work[16] = {}, info = -1;
  gesdd(&job, &size, &size, diagonal, &leading, singular, unused_u, &unused_leading, unused_vt, &unused_leading, &query, &query_length, integer_work, &info);
  require(info == 0 && query >= 1 && query < 1024 * 1024, "ILP64 SVD workspace query");
  const int64_t workspace_length = static_cast<int64_t>(query);
  std::vector<double> workspace(static_cast<size_t>(workspace_length));
  const double initial[] = {3, 0, 0, 2};
  std::memcpy(diagonal, initial, sizeof(initial));
  gesdd(&job, &size, &size, diagonal, &leading, singular, unused_u, &unused_leading, unused_vt, &unused_leading, workspace.data(), &workspace_length, integer_work, &info);
  require(info == 0 && singular[0] == 3.0 && singular[1] == 2.0, "ILP64 SVD result");
  record_maps();
}

void geometry(const fs::path &root) {
  Library dependency(root / "libgeos-9c5cb28e.so.3.13.1");
  Library api(root / "libgeos_c-42bdf9a9.so.1.19.2");
  require(std::strcmp(api.get<const char *(*)()>("GEOSversion")(), "3.13.1-CAPI-1.19.2") == 0, "GEOS version");
  void *context = api.get<void *(*)()>("GEOS_init_r")();
  require(context != nullptr, "GEOS context");
  auto sequence = api.get<void *(*)(void *, unsigned, unsigned)>("GEOSCoordSeq_create_r")(context, 2, 2);
  require(sequence != nullptr, "GEOS sequence");
  using Set = int (*)(void *, void *, unsigned, double);
  auto set_x = api.get<Set>("GEOSCoordSeq_setX_r"), set_y = api.get<Set>("GEOSCoordSeq_setY_r");
  require(set_x(context, sequence, 0, 0) == 1 && set_y(context, sequence, 0, 0) == 1 && set_x(context, sequence, 1, 6) == 1 && set_y(context, sequence, 1, 8) == 1, "GEOS coordinates");
  void *line = api.get<void *(*)(void *, void *)>("GEOSGeom_createLineString_r")(context, sequence);
  require(line != nullptr, "GEOS line");
  using Get = int (*)(void *, void *, double *);
  double length = 0, x = 0, y = 0;
  require(api.get<Get>("GEOSLength_r")(context, line, &length) == 1 && length == 10, "GEOS length");
  void *point = api.get<void *(*)(void *, void *, double)>("GEOSInterpolate_r")(context, line, 5);
  require(point != nullptr, "GEOS interpolation");
  require(api.get<Get>("GEOSGeomGetX_r")(context, point, &x) == 1 && api.get<Get>("GEOSGeomGetY_r")(context, point, &y) == 1 && x == 3 && y == 4, "GEOS interpolation result");
  auto destroy = api.get<void (*)(void *, void *)>("GEOSGeom_destroy_r");
  destroy(context, point);
  destroy(context, line);
  api.get<void (*)(void *)>("GEOS_finish_r")(context);
  record_maps();
}

void brotli(const fs::path &root) {
  Library common(root / "libbrotlicommon.so.1"), encoder(root / "libbrotlienc.so.1"), decoder(root / "libbrotlidec.so.1");
  require(encoder.get<uint32_t (*)()>("BrotliEncoderVersion")() == 16781312 && decoder.get<uint32_t (*)()>("BrotliDecoderVersion")() == 16781312, "Brotli1.1.0 version");
  const std::string input = "Source-pinned foreign Brotli component: native round trip.";
  size_t encoded_size = encoder.get<size_t (*)(size_t)>("BrotliEncoderMaxCompressedSize")(input.size());
  require(encoded_size > 0 && encoded_size < 1024 * 1024, "Brotli bound");
  std::vector<uint8_t> encoded(encoded_size), decoded(input.size());
  using Compress = int (*)(int, int, int, size_t, const uint8_t *, size_t *, uint8_t *);
  require(encoder.get<Compress>("BrotliEncoderCompress")(11, 22, 0, input.size(), reinterpret_cast<const uint8_t *>(input.data()), &encoded_size, encoded.data()) == 1 && encoded_size <= encoded.size(), "Brotli compression");
  size_t decoded_size = decoded.size();
  using Decompress = int (*)(size_t, const uint8_t *, size_t *, uint8_t *);
  require(decoder.get<Decompress>("BrotliDecoderDecompress")(encoded_size, encoded.data(), &decoded_size, decoded.data()) == 1 && decoded_size == input.size() && std::memcmp(decoded.data(), input.data(), input.size()) == 0, "Brotli round trip");
  record_maps();
}

void acados(const fs::path &root) {
  // No old project-generated solver is present or called. Verify only the core
  // foreign C entry points used by the current Rust planner's generated solver.
  Library api(root / "libacados.so");
  for (const char *symbol : {"ocp_nlp_cost_model_set", "ocp_nlp_constraints_model_set", "ocp_nlp_out_set", "ocp_nlp_out_get", "ocp_nlp_dims_get_from_attr", "ocp_nlp_cost_dims_get_from_attr", "ocp_nlp_constraint_dims_get_from_attr", "ocp_nlp_eval_cost", "ocp_nlp_get"}) {
    api.get<void (*)()>(symbol);
  }
  require(!fs::exists(root / "libacados_ocp_solver_lat.so") && !fs::exists(root / "libacados_ocp_solver_long.so") && !fs::exists(root / "manifest.json"), "partial acados libraries must not be a complete solver artifact");
  record_maps();
}

int main(int argc, char **argv) {
  try {
    require(argc == 6, "numerics/geos/brotli/acados directories and maps output required");
    maps_output = argv[5];
    setenv("OPENBLAS_NUM_THREADS", "1", 1);
    setenv("OMP_NUM_THREADS", "1", 1);
    numerics(argv[1]);
    geometry(argv[2]);
    brotli(argv[3]);
    acados(argv[4]);
    std::cout << "{\"numerics_dot_matrix_svd\":true,\"geos_interpolation\":true,\"brotli_roundtrip\":true,\"acados_core_symbols\":true,\"mpc_solver_tested\":false}\n";
    return 0;
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
