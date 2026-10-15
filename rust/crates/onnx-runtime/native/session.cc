// Thin external ORT C ABI adapter. Model interpretation stays in Rust.
#include "openpilot-onnx-runtime/src/bridge.rs.h"
#include "onnxruntime_c_api.h"
#include <dlfcn.h>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>
namespace openpilot_ort {
struct Session::Impl {
  void* library=nullptr;
  const OrtApi* api=nullptr;
  OrtEnv* environment=nullptr;
  OrtSession* session=nullptr;
  ~Impl() {
    if (api && session) api->ReleaseSession(session);
    if (api && environment) api->ReleaseEnv(environment);
    if (library) dlclose(library);
  }
  void check(OrtStatus* status) const {
    if (!status) return;
    std::string error=api->GetErrorMessage(status);
    api->ReleaseStatus(status);
    throw std::runtime_error(error);
  }
};
Session::Session(std::unique_ptr<Impl> object):value(std::move(object)){}
Session::~Session()=default;
static std::string text(rust::Str value) {
  std::string result(value.data(),value.size());
  if (result.find('\0')!=std::string::npos) throw std::invalid_argument("NUL in ORT argument");
  return result;
}
std::unique_ptr<Session> load(rust::Str library_path, rust::Str model_path) {
  auto state=std::make_unique<Session::Impl>();
  const auto library=text(library_path), model=text(model_path);
  state->library=dlopen(library.c_str(),RTLD_NOW|RTLD_LOCAL);
  if (!state->library) throw std::runtime_error(dlerror());
  void* symbol=dlsym(state->library,"OrtGetApiBase");
  if (!symbol) throw std::runtime_error("ORT API symbol absent");
  decltype(&OrtGetApiBase) get_api=nullptr;
  static_assert(sizeof(get_api)==sizeof(symbol));
  std::memcpy(&get_api,&symbol,sizeof(symbol));
  const auto* base=get_api();
  if (std::strcmp(base->GetVersionString(),"1.23.2")!=0) throw std::runtime_error("ORT version mismatch");
  state->api=base->GetApi(ORT_API_VERSION);
  if (!state->api) throw std::runtime_error("ORT API version unavailable");
  const auto* api=state->api;
  state->check(api->CreateEnv(ORT_LOGGING_LEVEL_WARNING,"signal",&state->environment));
  OrtSessionOptions* raw_options=nullptr;
  state->check(api->CreateSessionOptions(&raw_options));
  const auto release_options=[api](OrtSessionOptions* p){api->ReleaseSessionOptions(p);};
  std::unique_ptr<OrtSessionOptions,decltype(release_options)> options(raw_options,release_options);
  state->check(api->SetIntraOpNumThreads(options.get(),1));
  state->check(api->SetInterOpNumThreads(options.get(),1));
  state->check(api->AddSessionConfigEntry(options.get(),"session.intra_op.allow_spinning","0"));
  // No EP is appended: the C API's default is CPUExecutionProvider.
  state->check(api->CreateSession(state->environment,model.c_str(),options.get(),&state->session));
  return std::make_unique<Session>(std::move(state));
}
rust::Vec<Tensor> run(Session& owner, rust::Str requested_name, rust::Slice<const float> input, rust::Slice<const std::int64_t> shape) {
  auto& state=*owner.value;
  const auto* api=state.api;
  const auto name=text(requested_name);
  std::size_t count=1;
  if (shape.empty()) throw std::invalid_argument("empty ORT input shape");
  for (auto dim:shape) {
    if (dim<=0 || static_cast<std::uint64_t>(dim)>std::numeric_limits<std::size_t>::max()/count) throw std::invalid_argument("invalid ORT input shape");
    count*=static_cast<std::size_t>(dim);
  }
  if (count!=input.size() || count>std::numeric_limits<std::size_t>::max()/sizeof(float)) throw std::invalid_argument("ORT input length mismatch");
  std::size_t input_count=0,output_count=0;
  state.check(api->SessionGetInputCount(state.session,&input_count));
  state.check(api->SessionGetOutputCount(state.session,&output_count));
  if (input_count!=1 || output_count==0) throw std::invalid_argument("unsupported ORT input/output count");
  OrtAllocator* allocator=nullptr;
  state.check(api->GetAllocatorWithDefaultOptions(&allocator));
  struct Names {
    OrtAllocator* allocator; std::vector<char*> values;
    ~Names(){for(auto* p:values) allocator->Free(allocator,p);}
  } names{allocator,{}};
  char* actual_name=nullptr;
  state.check(api->SessionGetInputName(state.session,0,allocator,&actual_name));
  names.values.push_back(actual_name);
  if (name!=actual_name) throw std::invalid_argument("ORT input name mismatch");
  for(std::size_t i=0;i<output_count;++i) {
    char* output=nullptr;
    state.check(api->SessionGetOutputName(state.session,i,allocator,&output));
    names.values.push_back(output);
  }
  OrtMemoryInfo* raw_memory=nullptr;
  state.check(api->CreateCpuMemoryInfo(OrtArenaAllocator,OrtMemTypeDefault,&raw_memory));
  const auto release_memory=[api](OrtMemoryInfo* p){api->ReleaseMemoryInfo(p);};
  std::unique_ptr<OrtMemoryInfo,decltype(release_memory)> memory(raw_memory,release_memory);
  OrtValue* raw_input=nullptr;
  state.check(api->CreateTensorWithDataAsOrtValue(memory.get(),const_cast<float*>(input.data()),count*sizeof(float),shape.data(),shape.size(),ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT,&raw_input));
  const auto release_value=[api](OrtValue* p){api->ReleaseValue(p);};
  std::unique_ptr<OrtValue,decltype(release_value)> tensor(raw_input,release_value);
  std::vector<const char*> output_names;
  for(std::size_t i=1;i<names.values.size();++i) output_names.push_back(names.values[i]);
  struct Values {
    const OrtApi* api; std::vector<OrtValue*> values;
    ~Values(){for(auto* p:values) if(p) api->ReleaseValue(p);}
  } outputs{api,std::vector<OrtValue*>(output_count,nullptr)};
  const OrtValue* inputs[]={tensor.get()};
  const char* input_names[]={actual_name};
  state.check(api->Run(state.session,nullptr,input_names,inputs,1,output_names.data(),output_count,outputs.values.data()));
  rust::Vec<Tensor> results;
  for(auto* output:outputs.values) {
    OrtTensorTypeAndShapeInfo* raw_info=nullptr;
    state.check(api->GetTensorTypeAndShape(output,&raw_info));
    const auto release_info=[api](OrtTensorTypeAndShapeInfo* p){api->ReleaseTensorTypeAndShapeInfo(p);};
    std::unique_ptr<OrtTensorTypeAndShapeInfo,decltype(release_info)> info(raw_info,release_info);
    ONNXTensorElementDataType type;
    state.check(api->GetTensorElementType(info.get(),&type));
    if(type!=ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT) throw std::invalid_argument("ORT output is not Float32");
    std::size_t dimensions=0,elements=0;
    state.check(api->GetDimensionsCount(info.get(),&dimensions));
    state.check(api->GetTensorShapeElementCount(info.get(),&elements));
    std::vector<std::int64_t> dims(dimensions);
    state.check(api->GetDimensions(info.get(),dims.data(),dims.size()));
    void* raw_data=nullptr;
    state.check(api->GetTensorMutableData(output,&raw_data));
    Tensor result;
    for(auto dim:dims) result.shape.push_back(dim);
    const auto* data=static_cast<const float*>(raw_data);
    result.values.reserve(elements);
    for(std::size_t i=0;i<elements;++i) result.values.push_back(data[i]);
    results.push_back(std::move(result));
  }
  return results;
}
}
