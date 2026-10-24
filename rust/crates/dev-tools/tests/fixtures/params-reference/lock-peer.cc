// Controlled child only: validate the checker's real advisory lock and cleanup.
#include <sys/file.h>
#include <fcntl.h>
#include <unistd.h>
#include <cstdlib>
#include <fstream>
#include <iterator>
#include <iostream>
#include <string>
int main(int argc,char**argv) {
  if (argc!=5) return 2;
  std::string root=argv[1];
  std::ofstream(root+"/pid")<<getpid();
  std::string mode=std::getenv("PARAMS_LOCK_PEER_MODE")?std::getenv("PARAMS_LOCK_PEER_MODE"):"pass";
  std::string value((std::istreambuf_iterator<char>(std::cin)),{});
  if (mode=="unblocked") return 0;
  int fd=open((root+"/.lock").c_str(),O_RDONLY);
  if(fd<0 || flock(fd,LOCK_EX)) return 3;
  if (mode=="hang") { for (;;) pause(); }
  if (mode=="exit") return 7;
  std::ofstream(root+"/d/CarParams",std::ios::binary)<<value;
  return 0;
}
