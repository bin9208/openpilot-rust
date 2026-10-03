int main(int argc, char **argv) {
  assert(argc == 9);
  directory = argv[1];
  std::filesystem::create_directories(directory);
  trace.open(directory / "trace.tsv");
  const std::string kind = argv[2];
  const int width = std::stoi(argv[3]), height = std::stoi(argv[4]);
  const int output_width = std::stoi(argv[5]), output_height = std::stoi(argv[6]);
  const size_t stride = std::stoul(argv[7]);
  const unsigned frames = std::stoul(argv[8]);
  const size_t uv_offset = stride * height + stride * 4;
  std::vector<uint8_t> pixels(uv_offset + stride * height / 2);
  VisionBuf buffer{pixels.data(), pixels.data() + uv_offset, size_t(width), size_t(height), stride};
  std::unique_ptr<FfmpegEncoder> codec;
  std::unique_ptr<JpegEncoder> jpeg;
  if (kind == "jpeg") jpeg = std::make_unique<JpegEncoder>("thumbnail", output_width, output_height);
  else {
    EncoderInfo info;
    info.width = output_width; info.height = output_height;
    info.settings.encode_type = kind == "h264" ? cereal::EncodeIndex::Type::QCAMERA_H264 : cereal::EncodeIndex::Type::BIG_BOX_LOSSLESS;
    codec = std::make_unique<FfmpegEncoder>(info, width, height);
  }
  for (unsigned segment = 0; segment < 2; ++segment) {
    if (codec) codec->encoder_open();
    for (unsigned offset = 0; offset < frames; ++offset) {
      const uint32_t frame_id = 100 + segment * frames + offset;
      for (size_t index = 0; index < pixels.size(); ++index) pixels[index] = (index * 17 + (index / stride) * 13 + frame_id * 7) % 256;
      VisionIpcBufExtra extra{frame_id, 1000000000ULL + uint64_t(frame_id) * 50000000ULL, 1020000000ULL + uint64_t(frame_id) * 50000000ULL};
      if (codec) {
        const int result = codec->encode_frame(&buffer, &extra);
        trace << "R\t" << frame_id << '\t' << result << '\n';
      }
      if (jpeg) jpeg->pushThumbnail(&buffer, extra);
    }
    if (codec) codec->encoder_close();
  }
}
