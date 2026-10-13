int main(int argc,char **argv) {
  if(argc!=7)return 2;
  ClusterH264EncoderConfig config;
  config.width=std::stoi(argv[2]);config.height=std::stoi(argv[3]);config.fps=10;config.bitrate=2000000;config.gop=1;
  config.rate_control=std::stoi(argv[4]);config.slice_max_bytes=std::stoi(argv[5]);config.realtime_priority=std::string(argv[6])=="1";
  const std::string mode=argv[1];
  try {
    ClusterH264Encoder encoder(config);encoder.open();
    const size_t size=std::max({encoder.input_sizeimage(),encoder.input_active_bytes(),size_t(std::get<3>(get_nv12_info(config.width,config.height)))});
    std::cout<<"OPEN "<<config.width<<' '<<config.height<<' '<<encoder.input_stride()<<' '<<encoder.input_y_scanlines()<<' '<<encoder.input_uv_scanlines()<<' '<<encoder.input_uv_offset()<<' '<<size<<' '<<encoder.input_sizeimage()<<' '<<encoder.capture_sizeimage()<<'\n';
    auto packet=[](const ClusterH264PacketView &packet) {
      std::cout<<"PACKET "<<packet.timestamp_us<<' '<<packet.flags<<' '<<int(packet.codec_config)<<' '<<int(packet.keyframe)<<' ';
      const char *digits="0123456789abcdef";
      for(size_t i=0;i<packet.size;++i)std::cout<<digits[packet.data[i]>>4]<<digits[packet.data[i]&15];
      std::cout<<'\n';
    };
    for(unsigned frame=0;frame<12;++frame) {
      std::vector<uint8_t> bytes(size,uint8_t(frame));const uint64_t timestamp=frame==11?UINT64_MAX:uint64_t(frame)*100000+99;
      if(mode=="active")encoder.encode_nv12_active(bytes.data(),encoder.input_active_bytes(),timestamp,packet);
      else if(mode=="short")encoder.encode_nv12(bytes.data(),size-1,timestamp,packet);
      else if(mode=="acquire" || mode=="dmabuf" || mode=="cancel") {
        auto input=encoder.acquire_nv12_input(packet);
        if(mode=="cancel") {
          encoder.cancel_nv12_input(input.index);
          try {encoder.cancel_nv12_input(input.index);abort();}catch(const std::exception &){}
          input=encoder.acquire_nv12_input(packet);
        }
        memset(input.data,frame,input.size);
        if(mode=="dmabuf")encoder.submit_nv12_input_dmabuf(input.index,timestamp,packet);
        else encoder.submit_nv12_input(input.index,timestamp,packet);
      } else encoder.encode_nv12(bytes.data(),size,timestamp,packet);
      const auto &timing=encoder.last_encode_timings();
      if(timing.total_us!=timing.pre_poll_us+timing.wait_input_us+timing.convert_us+timing.sync_us+timing.queue_us+timing.post_poll_us)abort();
    }
    encoder.drain(0,packet);encoder.close();encoder.close();if(encoder.is_open())abort();
    std::cout<<"CLOSED\n";
  } catch(const std::exception &error) {std::cout<<"ERROR "<<error.what()<<"\nCLOSED\n";}
}
