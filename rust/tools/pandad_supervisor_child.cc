#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dirent.h>
#include <fstream>
#include <limits.h>
#include <sstream>
#include <unistd.h>
#include <vector>

static volatile sig_atomic_t caught = 0;
static void receive(int signal) { caught = signal; }
static void bytes(std::ostream &output, const char *text) {
  output << '[';
  for (size_t i = 0; text[i]; ++i) {
    if (i) output << ',';
    output << static_cast<unsigned>(static_cast<unsigned char>(text[i]));
  }
  output << ']';
}
static bool report(int argc, char **argv, int phase) {
  DIR *directory = opendir("/proc/self/fd");
  if (!directory) return false;
  const int own = dirfd(directory);
  std::vector<int> descriptors;
  while (dirent *entry = readdir(directory)) {
    const int fd = std::atoi(entry->d_name);
    if (fd > 2 && fd != own) descriptors.push_back(fd);
  }
  closedir(directory);
  char current[PATH_MAX];
  if (!getcwd(current, sizeof(current))) return false;
  const char *path = std::getenv("PANDA_CHILD_TRACE");
  if (!path) return false;
  std::ostringstream output;
  output << "{\"pid\":" << getpid() << ",\"phase\":" << phase << ",\"args\":[";
  for (int i = 1; i < argc; ++i) { if (i > 1) output << ','; bytes(output, argv[i]); }
  output << "],\"cwd\":";
  bytes(output, current);
  output << ",\"manager\":" << (std::getenv("MANAGER_DAEMON") && std::strcmp(std::getenv("MANAGER_DAEMON"), "pandad") == 0 ? "true" : "false");
  output << ",\"extra_fds\":[";
  for (size_t i = 0; i < descriptors.size(); ++i) { if (i) output << ','; output << descriptors[i]; }
  output << "]}\n";
  std::ofstream file(path, std::ios::app);
  file << output.str();
  file.flush();
  return static_cast<bool>(file);
}
int main(int argc, char **argv) {
  struct sigaction action {};
  action.sa_handler = receive;
  sigemptyset(&action.sa_mask);
  if (sigaction(SIGINT, &action, nullptr) != 0 || !report(argc, argv, 0)) return 90;
  const char *mode = std::getenv("PANDA_CHILD_MODE");
  if (mode && std::strcmp(mode, "hold") == 0) {
    sigset_t blocked, previous;
    sigemptyset(&blocked);
    sigaddset(&blocked, SIGINT);
    if (sigprocmask(SIG_BLOCK, &blocked, &previous) != 0) return 91;
    while (!caught) sigsuspend(&previous);
    if (sigprocmask(SIG_SETMASK, &previous, nullptr) != 0 || !report(argc, argv, caught)) return 92;
  }
  const char *code = std::getenv("PANDA_CHILD_CODE");
  return code ? std::atoi(code) : 0;
}
