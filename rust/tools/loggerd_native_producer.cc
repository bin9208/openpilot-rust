// Native host QA drives the original FfmpegEncoder and VideoEncoder publisher.
#include <iostream>
#include <string>
#include <vector>
#include "system/loggerd/encoder/ffmpeg_encoder.h"

int main(int argc, char **argv) {
  if (argc != 2) return 2;
  const EncoderInfo &info = std::string(argv[1]) == "qroad" ? qcam_encoder_info : main_road_encoder_info;
  constexpr int width = 160, height = 120;
  std::vector<uint8_t> pixels(width * height * 3 / 2, 128);
  VisionBuf buffer;
  buffer.width = width;
  buffer.height = height;
  buffer.stride = width;
  buffer.y = pixels.data();
  buffer.uv = pixels.data() + width * height;
  FfmpegEncoder encoder(info, width, height);
  encoder.encoder_open();
  std::cout << "ready" << std::endl;
  std::string command;
  while (std::cin >> command) {
    if (command == "next") {
      encoder.encoder_close();
      encoder.encoder_open();
    } else if (command == "frame") {
      uint32_t frame;
      if (!(std::cin >> frame)) return 3;
      for (int y = 0; y < height; ++y) {
        for (int x = 0; x < width; ++x) pixels[y * width + x] = (x + y + frame) % 256;
      }
      VisionIpcBufExtra extra = {};
      extra.frame_id = frame;
      extra.timestamp_sof = 1000000000ULL + frame * 50000000ULL;
      extra.timestamp_eof = extra.timestamp_sof + 1000000ULL;
      encoder.encode_frame(&buffer, &extra);
    } else if (command == "quit") {
      break;
    } else {
      return 4;
    }
    std::cout << "ok" << std::endl;
  }
  return 0;
}
