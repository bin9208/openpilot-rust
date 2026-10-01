#include <signal.h>
#include <unistd.h>
static void record_signal(int signal_number) {
  const char *message = signal_number == SIGUSR1 ? "check\n" : "download\n";
  const size_t length = signal_number == SIGUSR1 ? 6 : 9;
  ssize_t written = write(STDOUT_FILENO, message, length);
  if (written != (ssize_t)length) _exit(2);
}
int main(int argc, char **argv) {
  struct sigaction action = {0};
  action.sa_handler = record_signal;
  sigemptyset(&action.sa_mask);
  if (sigaction(SIGUSR1, &action, 0) || sigaction(SIGHUP, &action, 0)) return 1;
  if (write(STDOUT_FILENO, "ready\n", 6) != 6) return 2;
  for (;;) {
    char command;
    ssize_t count = read(STDIN_FILENO, &command, 1);
    if (count == 0) return 0;
    if (count == 1 && command == 'X' && argc > 1) {
      char *arguments[] = {argv[1], 0};
      execv(argv[1], arguments);
      return 3;
    }
  }
}
