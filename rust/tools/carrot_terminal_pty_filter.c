/* Owned fixture: inject only unsupported TIOCGPTPEER in this exec's descendants. */
#include <errno.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/prctl.h>
#include <sys/syscall.h>
#include <unistd.h>

int main(int argc, char **argv) {
  if (argc < 2) return 2;
  struct sock_filter filter[] = {
    BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr)),
    BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, __NR_ioctl, 0, 3),
    BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args) + sizeof(unsigned long long)),
    BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, TIOCGPTPEER, 0, 1),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | ENOTTY),
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
  };
  struct sock_fprog program = {sizeof(filter) / sizeof(filter[0]), filter};
  if (prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) || prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &program)) {
    perror("owned PTY seccomp");
    return 1;
  }
  fprintf(stderr, "[owned-pty-filter] injecting ENOTTY only for TIOCGPTPEER=0x%x\n", TIOCGPTPEER);
  execvp(argv[1], argv + 1);
  perror("owned PTY fixture exec");
  return 1;
}
