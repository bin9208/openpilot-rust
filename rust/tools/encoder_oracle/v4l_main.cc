int main(int argc, char **argv) {
  assert(argc == 8);
  directory = argv[1]; std::filesystem::create_directories(directory); trace.open(directory / "trace.tsv");
  const std::string mode = argv[2];
  const int camera = std::stoi(argv[3]), encoder_index = std::stoi(argv[4]);
  const int width = std::stoi(argv[5]), height = std::stoi(argv[6]);
  const unsigned frames = std::stoul(argv[7]);
  EncoderInfo info;
  if (mode == "main") {
    const char *services[] = {"roadEncodeData", "driverEncodeData", "wideRoadEncodeData"};
    info.publish_name = services[camera];
    info.get_settings = EncoderSettings::MainEncoderSettings;
    if (encoder_index == 1) {
      info.publish_name = "qRoadEncodeData"; info.frame_width = 526; info.frame_height = 330;
      info.get_settings = [](int) { return EncoderSettings::QcamEncoderSettings(); };
    }
  } else if (mode == "--stream" || mode == "--carrot-vision-road") {
    const char *services[] = {"livestreamRoadEncodeData", "livestreamDriverEncodeData", "livestreamWideRoadEncodeData"};
    info.publish_name = services[camera]; info.get_settings = [](int) { return EncoderSettings::StreamEncoderSettings(); };
  } else {
    info.publish_name = "youtubeRoadEncodeData";
    if (mode == "--youtube-low") info.get_settings = [](int) { return EncoderSettings::YouTubeLowEncoderSettings(); };
    else if (mode == "--youtube-medium") info.get_settings = [](int) { return EncoderSettings::YouTubeMediumEncoderSettings(); };
    else if (mode == "--youtube-wide") info.get_settings = [](int) { return EncoderSettings::YouTubeWideEncoderSettings(); };
    else info.get_settings = [](int) { return EncoderSettings::YouTubeEncoderSettings(); };
  }
  V4LEncoder encoder(info, width, height);
  encoder.encoder_open();
  std::vector<VisionBuf> inputs;
  for (unsigned segment = 0; segment < 2; ++segment) {
    for (unsigned frame = 0; frame < frames; ++frame) {
      const uint32_t frame_id = 100 + segment * frames + frame;
      VisionBuf input;
      input.len = size_t(width) * height * 3 / 2; input.mmap_len = input.len;
      input.fd = syscall(SYS_memfd_create, "encoder-input-fixture", 0); assert(input.fd >= 0);
      assert(ftruncate(input.fd, input.len) == 0);
      input.addr = mmap(nullptr, input.len, PROT_READ | PROT_WRITE, MAP_SHARED, input.fd, 0); assert(input.addr != MAP_FAILED);
      for (size_t index = 0; index < input.len; ++index) static_cast<uint8_t *>(input.addr)[index] = (index * 17 + frame_id * 7) % 256;
      VisionIpcBufExtra extra{frame_id, 1000000000ULL + uint64_t(frame_id) * 50000000ULL, 1020000000ULL + uint64_t(frame_id) * 50000000ULL};
      assert(encoder.encode_frame(&input, &extra) == int(frame));
      inputs.push_back(input);
    }
    encoder.set_idle(true); encoder.encoder_close();
    if (segment == 0) encoder.encoder_open();
    for (const auto &input : inputs) { munmap(input.addr, input.mmap_len); close(input.fd); }
    inputs.clear(); encoder.set_idle(false);
  }
}
