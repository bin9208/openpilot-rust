#pragma once
#include "rust/cxx.h"
#include <memory>
#include <cstdint>
namespace startup_ui {
class GlMapping;
class GlesApi {
public:
  explicit GlesApi(rust::Str path);
  ~GlesApi();
  int32_t integer(uint32_t parameter) const;
  uint32_t gl_error() const;
  void bind_framebuffer(uint32_t target,uint32_t name) const;
  void bind_buffer(uint32_t target,uint32_t name) const;
  void pixel_store(uint32_t parameter,int32_t value) const;
  void read_rgba(int32_t width,int32_t height,rust::Slice<uint8_t> destination) const;
  void read_rgba_pbo(int32_t width,int32_t height) const;
  bool async_available() const;
  void prepare_dma() const;
  rust::Vec<uint32_t> generate_buffers(uint32_t count) const;
  void delete_buffers(rust::Slice<const uint32_t> names) const;
  void allocate_buffer(uint32_t target,uint64_t size,uint32_t usage) const;
  uint64_t fence(uint32_t condition,uint32_t flags) const;
  uint32_t wait_fence(uint64_t fence,uint32_t flags,uint64_t timeout) const;
  void delete_fence(uint64_t fence) const;
  void flush() const;
  std::unique_ptr<GlMapping> map_read(uint32_t target,uint64_t size) const;
  uint32_t generate_texture() const;
  void delete_texture(uint32_t texture) const;
  void bind_texture(uint32_t target,uint32_t texture) const;
  void texture_parameter(uint32_t target,uint32_t parameter,int32_t value) const;
  uint32_t generate_framebuffer() const;
  void delete_framebuffer(uint32_t framebuffer) const;
  void attach_texture(uint32_t target,uint32_t attachment,uint32_t texture) const;
  uint32_t framebuffer_status(uint32_t target) const;
private:
  struct State;
  std::shared_ptr<State> state;
  uint64_t read_extent(int32_t width,int32_t height) const;
  friend class GlMapping;
};
class GlMapping {
public:
  GlMapping(std::shared_ptr<GlesApi::State> api,uint32_t target,uint64_t size);
  ~GlMapping();
  void copy_to(rust::Slice<uint8_t> destination) const;
  bool unmap();
private:
  std::shared_ptr<GlesApi::State> api;
  uint32_t target;
  uint32_t buffer;
  uint64_t size;
  void *address;
};
std::unique_ptr<GlesApi> gles_api(rust::Str path);
}
