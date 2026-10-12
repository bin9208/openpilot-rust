int main(int argc,char **argv) {
  if(argc!=2)return 2;
  const std::string scenario=argv[1];
  ClusterH264DecoderConfig config{.width=96,.height=64,.fps=30,.device_path="/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index0",.debug=false};
  ClusterH264Decoder decoder(config);
  std::vector<int> retained;
  try {
    decoder.open();decoder.open();
    std::cout << "OPEN\n";
    const std::array<uint64_t,5> sequences{0,1,1234567,UINT64_MAX,99};
    for(size_t step=0;step<sequences.size();++step) {
      std::array<uint8_t,16> packet{};
      for(size_t index=0;index<packet.size();++index)packet[index]=uint8_t(index+step*7);
      ClusterH264DecodedFrame frame;
      const bool found=decoder.decode(packet.data(),packet.size(),sequences[step],250,&frame);
      if(!found){std::cout << "NONE\n";continue;}
      const int fd=dup(frame.fd);if(fd<0)abort();
      std::array<uint8_t,64> pixels{};if(pread(fd,pixels.data(),pixels.size(),0)!=ssize_t(pixels.size()))abort();
      std::cout << "FRAME " << frame.index << ' ' << frame.width << ' ' << frame.height << ' ' << frame.stride << ' ' << frame.uv_offset << ' ' << frame.sequence << ' ';
      const char *digits="0123456789abcdef";for(auto byte:pixels)std::cout << digits[byte>>4] << digits[byte&15];std::cout << '\n';
      if(scenario=="leased-port-change")retained.push_back(fd);
      else{close(fd);decoder.release(frame.index);}
    }
  } catch(const std::exception &error) {std::cout << "ERROR " << error.what() << '\n';}
  decoder.close();decoder.close();for(auto fd:retained)close(fd);
  std::cout << "CLOSED\n";
}
