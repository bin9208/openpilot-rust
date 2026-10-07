class EncoderInfo {
public:
  const char *publish_name;
  int frame_width = -1, frame_height = -1, fps = 20;
  std::function<EncoderSettings(int)> get_settings;
};
class PubMaster { public: explicit PubMaster(const std::vector<const char *> &) {} };
class VideoEncoder {
public:
  VideoEncoder(const EncoderInfo &info, int width, int height);
  virtual ~VideoEncoder() = default;
  virtual void set_idle(bool) {}
  void publisher_publish(int segment, uint32_t index, VisionIpcBufExtra &extra, unsigned flags,
      kj::ArrayPtr<capnp::byte> header, kj::ArrayPtr<capnp::byte> data) {
    trace << "P\t" << segment << '\t' << index << '\t' << extra.frame_id << '\t' << extra.timestamp_sof << '\t' << extra.timestamp_eof
          << '\t' << flags << '\t' << ((flags & 8) ? hex(header) : "") << '\t' << hex(data) << '\t' << out_width << '\t' << out_height
          << '\t' << count++ << '\t' << int(encoder_info.get_settings(in_width).encode_type) << '\n';
  }
protected:
  int in_width, in_height, out_width, out_height;
  const EncoderInfo encoder_info;
  std::unique_ptr<PubMaster> pm;
  unsigned count = 0;
};
