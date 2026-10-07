#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <type_traits>
#include <linux/ion.h>
#include <linux/msm_ion.h>
#include "msgq/msgq.h"
#include "msgq/visionipc/visionbuf.h"
#include "msgq/visionipc/visionipc.h"

#define FIELD(type, field) std::printf(#type "." #field "=%zu\n", offsetof(type, field))
#define VALUE(name) std::printf(#name "=%llu\n", static_cast<unsigned long long>(name))
#define SIZE(type) std::printf(#type "=%zu\n", sizeof(type))

int main(int argc, char **argv) {
  if (argc == 2 && std::strcmp(argv[1], "wire") == 0) {
    VisionBuf buffer{};
    static_assert(std::is_trivially_copyable_v<VisionBuf>);
    std::memset(static_cast<void *>(&buffer), 0, sizeof(buffer));
    buffer.len = 96;
    buffer.mmap_len = 104;
    buffer.addr = reinterpret_cast<void *>(0x1000);
    buffer.frame_id = reinterpret_cast<uint64_t *>(0x1060);
    buffer.fd = 9;
    buffer.width = 8;
    buffer.height = 4;
    buffer.stride = 16;
    buffer.uv_offset = 64;
    buffer.y = reinterpret_cast<uint8_t *>(0x1000);
    buffer.uv = reinterpret_cast<uint8_t *>(0x1040);
    buffer.server_id = 0x8877665544332211;
    buffer.idx = 3;
    buffer.type = VISION_STREAM_WIDE_ROAD;
    buffer.handle = 0;
    VisionIpcPacket packet{};
    static_assert(std::is_trivially_copyable_v<VisionIpcPacket>);
    std::memset(static_cast<void *>(&packet), 0, sizeof(packet));
    packet.server_id = 5;
    packet.idx = 3;
    packet.extra.frame_id = 7;
    packet.extra.timestamp_sof = 7000;
    packet.extra.timestamp_eof = 7100;
    packet.extra.valid = true;
    return std::fwrite(&buffer, sizeof(buffer), 1, stdout) != 1 || std::fwrite(&packet, sizeof(packet), 1, stdout) != 1;
  }
  SIZE(msgq_header_t);
  FIELD(msgq_header_t, num_readers);
  FIELD(msgq_header_t, write_pointer);
  FIELD(msgq_header_t, write_uid);
  FIELD(msgq_header_t, read_pointers);
  FIELD(msgq_header_t, read_valids);
  FIELD(msgq_header_t, read_uids);
  SIZE(VisionBuf);
  SIZE(VisionIpcPacket);
  SIZE(ion_allocation_data);
  SIZE(ion_fd_data);
  SIZE(ion_handle_data);
  SIZE(ion_custom_data);
  SIZE(ion_flush_data);
  FIELD(ion_allocation_data, len);
  FIELD(ion_allocation_data, align);
  FIELD(ion_allocation_data, heap_id_mask);
  FIELD(ion_allocation_data, flags);
  FIELD(ion_allocation_data, handle);
  FIELD(ion_fd_data, handle);
  FIELD(ion_fd_data, fd);
  FIELD(ion_custom_data, cmd);
  FIELD(ion_custom_data, arg);
  FIELD(ion_flush_data, handle);
  FIELD(ion_flush_data, fd);
  FIELD(ion_flush_data, vaddr);
  FIELD(ion_flush_data, offset);
  FIELD(ion_flush_data, length);
  VALUE(ION_IOC_ALLOC);
  VALUE(ION_IOC_SHARE);
  VALUE(ION_IOC_IMPORT);
  VALUE(ION_IOC_FREE);
  VALUE(ION_IOC_CUSTOM);
  VALUE(ION_IOC_CLEAN_CACHES);
  VALUE(ION_IOC_INV_CACHES);
  VALUE(ION_IOMMU_HEAP_ID);
  VALUE(ION_FLAG_CACHED);
}
