#pragma once
#include "rust/cxx.h"
#include <cstdint>
#include <memory>
namespace startup_ui {
  enum class ReferenceCore : std::uint8_t;
  enum class ReferenceExtension : std::uint8_t;
  // Raw ABI only. Rust owns initialization, fallback, retry and descriptor policy.
  class ReferenceApi {
    public:  ReferenceApi();
    ~ReferenceApi();
    void load_egl();
    void load_gles();
    void resolve_core(ReferenceCore);
    bool resolve_proc(ReferenceExtension);
    void resolve_library(ReferenceExtension);
    std::uint64_t reference_display();
    bool reference_initialize(std::uint64_t) const;
    rust::Vec<std::uint8_t> reference_extensions(std::uint64_t) const;
    std::int32_t reference_error() const;
    std::uint64_t reference_create(std::uint64_t,std::uint32_t,rust::Slice<const std::int32_t>);
    void reference_active(std::uint32_t) const;
    void reference_bind(std::uint32_t,std::uint32_t) const;
    void reference_image_target(std::uint32_t,std::uint64_t) const;
    bool reference_destroy(std::uint64_t,std::uint64_t);
    private:  struct State;
    std::unique_ptr<State> state;
  };
  std::unique_ptr<ReferenceApi> reference_api();
  std::int32_t reference_close_owned_descriptor(std::int32_t);
}
