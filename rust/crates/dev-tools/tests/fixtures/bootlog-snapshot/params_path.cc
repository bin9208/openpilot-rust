// Actual unchanged Params path boundary plus mechanical close-fds subprocess call.
// Snapshot/copy/thread policy belongs solely to the independent Rust adapter.
#include "common/params.h"
#include "common/swaglog.h"
#include <cstdarg>
#include <cstdio>
#include <cerrno>
#include <cstdlib>
#include <dirent.h>
#include <fcntl.h>
#include <sys/wait.h>
#include <unistd.h>
void cloudlog_e(int, const char *, int, const char *, const char *format, ...) {
  va_list args; va_start(args, format); vfprintf(stderr, format, args); va_end(args);
}
int main(int argc, char **argv) {
  if (argc == 2) { Params params; printf("%s", params.getParamPath().c_str()); return 0; }
  if (argc != 4 || std::string(argv[1]) != "call") return 2;
  int errors[2]; if (pipe2(errors, O_CLOEXEC)) return 3;
  pid_t child=fork(); if(child<0) return 4;
  if(child==0) {
    close(errors[0]);
    DIR *fds=opendir("/proc/self/fd");
    if (!fds) _exit(126);
    while (auto *entry=readdir(fds)) {
      int fd=atoi(entry->d_name);
      if(fd>2 && fd!=errors[1] && fd!=dirfd(fds)) close(fd);
    }
    closedir(fds);
    char *args[]={argv[2],nullptr};
    execv(argv[2],args);
    int error=errno;
    if(write(errors[1],&error,sizeof(error))!=sizeof(error)) _exit(126);
    _exit(127);
  }
  close(errors[1]); int error=0; ssize_t count;
  do {count=read(errors[0],&error,sizeof(error));} while(count<0 && errno==EINTR);
  close(errors[0]); int status; while(waitpid(child,&status,0)<0) if(errno!=EINTR) return 5;
  FILE *report=fopen(argv[3],"w"); if(!report) return 6;
  fprintf(report,"{\"exec_errno\":%d,\"returncode\":%d}",count==sizeof(error)?error:0,
    WIFEXITED(status)?WEXITSTATUS(status):-WTERMSIG(status));
  return fclose(report)==0?0:7;
}
