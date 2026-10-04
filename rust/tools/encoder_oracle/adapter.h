#include <cassert>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <memory>
#include <string>
#include <vector>
#include <libyuv.h>
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/imgutils.h>
#include <jpeglib.h>
}

#define LOGE(...) fprintf(stderr, __VA_ARGS__)
#define V4L2_BUF_FLAG_KEYFRAME 8
const int env_debug_encoder = 0;
namespace capnp { using byte = uint8_t; }
namespace kj {
template<class T> struct ArrayPtr { T *pointer; size_t length; };
template<class T> ArrayPtr<T> arrayPtr(T *pointer, size_t length) { return {pointer, length}; }
}
namespace cereal { struct EncodeIndex { enum class Type { QCAMERA_H264, BIG_BOX_LOSSLESS }; }; }
struct VisionIpcBufExtra { uint32_t frame_id; uint64_t timestamp_sof, timestamp_eof; };
struct VisionBuf { uint8_t *y, *uv; size_t width, height, stride; };
struct EncoderSettings { cereal::EncodeIndex::Type encode_type; };
struct EncoderInfo {
  const char *publish_name = "fixture";
  int width, height;
  int fps = 20;
  EncoderSettings settings;
  EncoderSettings get_settings(int) const { return settings; }
};
static std::filesystem::path directory;
static std::ofstream trace;
static unsigned packet_number = 0;
static void save_packet(const uint8_t *data, size_t size) {
  std::ofstream output(directory / ("packet-" + std::to_string(packet_number) + ".bin"), std::ios::binary);
  output.write(reinterpret_cast<const char *>(data), size);
}
class VideoEncoder {
public:
  VideoEncoder(const EncoderInfo &info, int width, int height)
      : in_width(width), in_height(height), out_width(info.width), out_height(info.height), encoder_info(info) {}
  virtual ~VideoEncoder() = default;
  void publisher_publish(int segment, uint32_t index, VisionIpcBufExtra &extra, unsigned flags,
      kj::ArrayPtr<capnp::byte>, kj::ArrayPtr<capnp::byte> data) {
    save_packet(data.pointer, data.length);
    trace << "P\t" << packet_number++ << '\t' << segment << '\t' << index << '\t' << extra.frame_id
          << '\t' << extra.timestamp_sof << '\t' << extra.timestamp_eof << '\t' << flags << '\t' << data.length << '\n';
  }
protected:
  int in_width, in_height, out_width, out_height;
  const EncoderInfo encoder_info;
};
struct ThumbnailMessage {
  uint32_t frame_id = 0;
  uint64_t timestamp = 0;
  kj::ArrayPtr<uint8_t> bytes{};
  void setFrameId(uint32_t value) { frame_id = value; }
  void setTimestampEof(uint64_t value) { timestamp = value; }
  void setThumbnail(kj::ArrayPtr<uint8_t> value) { bytes = value; }
};
struct ThumbnailBuilder {
  ThumbnailMessage *message;
  void setFrameId(uint32_t value) { message->setFrameId(value); }
  void setTimestampEof(uint64_t value) { message->setTimestampEof(value); }
  void setThumbnail(kj::ArrayPtr<uint8_t> value) { message->setThumbnail(value); }
};
struct EventBuilder { ThumbnailMessage *message; ThumbnailBuilder initThumbnail() { return {message}; } };
struct MessageBuilder { ThumbnailMessage message; EventBuilder initEvent() { return {&message}; } };
struct PubMaster {
  explicit PubMaster(const std::vector<const char *> &) {}
  void send(const char *, MessageBuilder &message) {
    auto &thumbnail = message.message;
    save_packet(thumbnail.bytes.pointer, thumbnail.bytes.length);
    trace << "J\t" << packet_number++ << '\t' << thumbnail.frame_id << '\t' << thumbnail.timestamp << '\t' << thumbnail.bytes.length << '\n';
  }
};
