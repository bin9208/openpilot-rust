#!/usr/bin/env bash
# #474 GHA-only adapter; the #560 provider, approval protocol and auditors stay unchanged.
set -euo pipefail
set +m
umask 077
recovery_host_path=${PATH:?}
export LC_ALL=C PATH=/usr/bin:/bin
fail() { printf 'recovery tracer 474: %s\n' "$*" >&2; exit 1; }
[[ $# == 1 ]] || fail 'prepare|controls|pty|trace|after required'
operation=$1
case $operation in prepare|controls|pty|trace|after) ;; *) fail 'unsupported operation';; esac
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-}:${GITHUB_JOB:-} == true:bin9208/openpilot-rust:recovery &&
   ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:X64 &&
   ${SOURCE_ALLOWED:-} == true && $(uname -s):$(uname -m) == Linux:x86_64 ]] || fail 'trusted hosted recovery required'
case ${GITHUB_EVENT_NAME:-} in push|workflow_dispatch) ;; *) fail 'unsupported source event';; esac
job=${CI_JOB_TEMP:?}; source=${DIAG_SOURCE:?}; owner=${DIAG_SOURCE_OWNER:?}; workspace=${DIAG_ACTIONS_WORKSPACE:?}
sdk=$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu
owned_dir() {
  [[ -d $1 && ! -L $1 && $(readlink -e -- "$1") == "$1" && $(stat -c '%u:%a' -- "$1") == "$(id -u):700" ]]
}
[[ $job == /tmp/openpilot-ci.?????? && $source == "$owner/source" && $PWD == "$source" &&
   $GITHUB_WORKSPACE == "$source" && ${CARGO_HOME:-} == "$job/cargo-home" &&
   ${RUSTUP_HOME:-} == "$job/rustup-home" && ${CI_PREPARED_RUST_ROOT:-} == "$sdk" &&
   ${CI_PREPARED_RUST_MANIFEST:-} == "$job/proof/prepared-rust-files.before.sha256" &&
   ${RUSTC:-} == "$sdk/bin/rustc" && ${RUSTDOC:-} == "$sdk/bin/rustdoc" &&
   ${CARGO_BUILD_JOBS:-}:${CARGO_INCREMENTAL:-}:${CARGO_PROFILE_DEV_DEBUG:-}:${CARGO_PROFILE_TEST_DEBUG:-} == 2:0:0:0 ]] || fail 'actual source/SDK/profile differs'
case $owner in "$workspace/.diag296-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."??????) ;; *) fail 'source owner namespace';; esac
for path in "$job" "$job/proof" "$source" "$owner"; do owned_dir "$path" || fail 'private owned directory required'; done
[[ $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(cat "$owner/.diag296-owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(stat -c '%d:%i' "$owner") == "$DIAG_SOURCE_IDENTITY" && $(git rev-parse HEAD) == "$GITHUB_SHA" ]] || fail 'owner/source binding differs'
unset GH_TOKEN GITHUB_TOKEN BASH_ENV ENV LD_PRELOAD LD_LIBRARY_PATH MAKEFLAGS MFLAGS CONFIG_SITE
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0
base=$job/proof/recovery-provider-474; work=$job/recovery-provider-474; root=$job/cold-strace-560
if [[ $operation == prepare ]]; then
  [[ ! -e $base && ! -L $base && ! -e $work && ! -L $work ]] || fail 'fresh provider adapter roots required'
  mkdir "$base" "$work"
  printf '%s\n' "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB:$GITHUB_SHA" > "$base/owner"
fi
owned_dir "$base" && owned_dir "$work" || fail 'owned prior adapter required; no cleanup of unknown roots'
[[ $(cat "$base/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB:$GITHUB_SHA" ]] || fail 'adapter owner differs'
out=$base/$operation; build=$work/$operation
mkdir "$out" "$out/logs" "$out/providers" "$build" "$build/tmp"
phase=preflight; passed=false; active= identity=; owned_code= owned_waited=; groups_ok=true
mode=run-inherited; launch_label= launch_dir= launch_fault= launch_tracer= launch_parent=; launch_seen=false
declare -A held_fd=() held_id=() held_sha=() aliases=() alias_id=() bound=()
extra_env=(); protocol_complete=false; provider_sha=
trap 'code=$?; printf "operation=%s\nphase=%s\noriginal_exit=%s\n" "$operation" "$phase" "$code" > "$out/early.status"' EXIT
[[ ${RECOVERY474_HELPER_SHA256:-} =~ ^[0-9a-f]{64}$ ]] || fail 'reviewed adapter SHA required'
printf '%s  %s\n' "$RECOVERY474_HELPER_SHA256" "$source/tools/ci/recovery_tracer_474.sh" | sha256sum --check --strict --quiet -

# Load only reviewed definitions from held FD bytes, never a whole diagnostic controller.
load_definitions() {
  local path=$1 digest=$2 before fd pid=$BASHPID
  [[ -f $path && ! -L $path && $(readlink -e "$path") == "$path" ]] || fail 'canonical definition file required'
  before=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path")
  exec {fd}<"$path"
  [[ $(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "/proc/$pid/fd/$fd") == "$before" ]] || fail 'definition FD identity'
  printf '%s  %s\n' "$digest" "/proc/$pid/fd/$fd" | sha256sum --check --strict --quiet -
  . "/proc/$pid/fd/$fd"
  [[ $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") == "$before" &&
     $(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "/proc/$pid/fd/$fd") == "$before" ]] || fail 'definitions changed while loading'
  exec {fd}<&-
}
launch_shared=$source/tools/ci/cold_tracer_launch_560.sh
load_definitions "$launch_shared" 1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a
COLD560_HELPER_SHA256=1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a
preparer=$source/tools/ci/prepare-cold-strace-560
export COLD560_PREPARE_HELPER_SHA256=051332a53de80a8debc38c7ef1fcaf0fa43a113d03300b33ea7e6660687f0c5c
printf '%s  %s\n' "$COLD560_PREPARE_HELPER_SHA256" "$preparer" | sha256sum --check --strict --quiet -
hold "$launch_shared"; hold "$preparer"; hold "$source/tools/ci/recovery_tracer_474.sh"; hold /usr/bin/timeout
[[ ${held_sha[$source/tools/ci/recovery_tracer_474.sh]} == "$RECOVERY474_HELPER_SHA256" &&
   ${held_sha[$preparer]} == "$COLD560_PREPARE_HELPER_SHA256" ]] || fail 'held source SHA differs'
export COLD560_PREPARE_ROOT=$root
export COLD560_PREPARE_CONTEXT=recovery474
export COLD560_NONPROVIDER_GROWTH_BYTES=${RECOVERY474_NONPROVIDER_GROWTH_BYTES:?source-authored remaining forecast required}
export COLD560_JOB_DEADLINE_EPOCH=${RECOVERY474_PROVIDER_DEADLINE_EPOCH:?actual first trust clock required}
cache=$job/openpilot-disk-guard
free_bytes() { df --output=avail -B1 "$1" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {n++;v=$1} END {if(n!=1)exit 1;print v}'; }
admit() {
  local allocated apparent available
  ((SECONDS<deadline)) || return 1
  allocated=$(du -sx --block-size=1 "$base" "$work" | awk '{n+=$1} END {printf "%.0f\n",n}') || return 1
  apparent=$(du -sx --apparent-size --block-size=1 "$base" "$work" | awk '{n+=$1} END {printf "%.0f\n",n}') || return 1
  available=$(free_bytes "$job") || return 1
  ((allocated<=256*1048576 && apparent<=256*1048576 && available>=256*1048576-allocated)) || return 1
  printf '%s\t%s\t%s\t%s\n' "$phase" "$allocated" "$apparent" "$available" >> "$out/admission.tsv"
}
finish() {
  local original=$? final=0 leader
  trap - EXIT INT TERM HUP; set +e
  if [[ -n $active ]]; then
    leader=$active
    if kill_owned; then wait -f "$leader"; else final=1; groups_ok=false; fi
    group_quiet "$leader" '' "$out/logs/final.group" || { final=1; groups_ok=false; }
    active= identity=
  fi
  verify_held || final=1
  if [[ $operation == trace ]]; then
    for item in stdout stderr; do
      [[ ! -f $out/logs/recovery.$item ]] || cp "$out/logs/recovery.$item" "$job/recovery-trace/harness.$item" || final=1
    done
    if [[ -f $out/logs/recovery.status ]]; then
      cp "$out/logs/recovery.status" "$job/recovery-trace/harness.status" || final=1
    else printf '125\n' > "$job/recovery-trace/harness.status"; final=1; fi
  fi
  [[ $original == 0 && $final == 0 && $groups_ok == true && $passed == true ]] || final=1
  printf 'original_exit=%s\nfinalizer_exit=%s\nowned_groups_nonlive=%s\n' "$original" "$final" "$groups_ok" > "$out/status"
  if ((final==0)); then (set -o noclobber; printf '%s\n' "$GITHUB_SHA" > "$base/$operation-complete") || final=1; fi
  ((final==0)) || exit 1
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
base_env=(PATH="$recovery_host_path" HOME="$job/home" LC_ALL=C TZ=UTC TMPDIR="$build/tmp" RUNNER_TEMP="$job"
  CARGO_HOME="$job/cargo-home" RUSTUP_HOME="$job/rustup-home" RUSTUP_TOOLCHAIN=1.94.0 CARGO_TARGET_DIR="$source/rust/target"
  RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  GITHUB_ACTIONS=true GITHUB_REPOSITORY=bin9208/openpilot-rust RUNNER_ENVIRONMENT="$RUNNER_ENVIRONMENT" RUNNER_OS="$RUNNER_OS" RUNNER_ARCH="$RUNNER_ARCH"
  GITHUB_RUN_ID="$GITHUB_RUN_ID" GITHUB_RUN_ATTEMPT="$GITHUB_RUN_ATTEMPT" GITHUB_JOB="$GITHUB_JOB" GITHUB_SHA="$GITHUB_SHA" RUNNER_NAME="$RUNNER_NAME"
  GITHUB_EVENT_NAME="$GITHUB_EVENT_NAME" SOURCE_ALLOWED="$SOURCE_ALLOWED" CI_JOB_TEMP="$job" GITHUB_WORKSPACE="$source"
  DIAG_SOURCE="$source" DIAG_SOURCE_OWNER="$owner" DIAG_SOURCE_IDENTITY="$DIAG_SOURCE_IDENTITY" DIAG_ACTIONS_WORKSPACE="$workspace"
  CI_PREPARED_RUST_ROOT="$sdk" CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST"
  OPENPILOT_CI_PREPARED_RUST_ROOT="$sdk" OPENPILOT_CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST"
  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0)
for name in ${!RECOVERY_FAMILY_@}; do base_env+=("$name=${!name}"); done
launch_strip=(-E COLD560_LOG -E COLD560_FILTER_INHERIT -E COLD560_LAUNCH_DIR -E COLD560_SOURCE_SHA -E COLD560_HELPER_SHA -E COLD560_PROVIDER_SHA)

if [[ $operation == prepare ]]; then
  phase=provider-scope-admission
  # Current recovery forecast: target6 + registry2 + proof1 + reserve0.5 GiB.
  [[ $COLD560_NONPROVIDER_GROWTH_BYTES == 10200547328 ]] || fail 'remaining recovery forecast changed without source review'
  printf 'context=recovery474\nprovider_bytes=536870912\nnonprovider_bytes=%s\nfast_default_envelope_bytes=6442450944\nscoped_envelope_bytes=10737418240\nouter_envelope_bytes=18253611008\n' \
    "$COLD560_NONPROVIDER_GROWTH_BYTES" > "$out/provider-budget.txt"
  [[ ! -e $cache && ! -L $cache ]] || fail 'provider must precede the first disk bootstrap'
  remaining=$((COLD560_JOB_DEADLINE_EPOCH-EPOCHSECONDS))
  ((remaining>0 && remaining<=420)) || fail 'original first-trust provider window expired/extended'
  deadline=$((SECONDS+remaining-20))
  # Source-authored negative context fixture. These invalid values must fail
  # before owner allocation/provider build; they do not describe the actual runner.
  for name in unknown-context wrong-job selfhosted-context; do
    extra_env=(COLD560_PREPARE_HELPER_SHA256="$COLD560_PREPARE_HELPER_SHA256" COLD560_PREPARE_CONTEXT=recovery474)
    expected_error='prepare cold strace 560: recovery474 context requires github-hosted recovery job'
    case $name in
      unknown-context)
        extra_env+=(COLD560_PREPARE_CONTEXT=unknown474)
        expected_error='prepare cold strace 560: unsupported preparation context';;
      wrong-job) extra_env+=(GITHUB_JOB=native);;
      selfhosted-context) extra_env+=(RUNNER_ENVIRONMENT=self-hosted);;
    esac
    start_owned "context-$name" 5 /usr/bin/bash "$preparer" build
    group_quiet "$active" "$active" "$out/logs/context-$name.group" || fail 'context rejection left live/unknown members'
    complete_owned "context-$name"
    [[ $owned_code == 1 && $owned_waited == 1 && ! -s $out/logs/context-$name.stdout ]] || fail 'invalid preparation context accepted/wrong exit'
    grep -Fx "$expected_error" "$out/logs/context-$name.stderr" >/dev/null || fail 'wrong preparation context rejection'
    [[ ! -e $root && ! -L $root && ! -e $job/proof/cold-strace-560 && ! -L $job/proof/cold-strace-560 &&
       ! -e $cache && ! -L $cache ]] || fail 'rejected context allocated a provider or warmed Cargo'
    printf '%s\tnegative-context\tPASS\n' "$name" >> "$out/context-controls.tsv"
  done
  printf 'unknown-context\tnegative-context\tPASS\nwrong-job\tnegative-context\tPASS\nselfhosted-context\tnegative-context\tPASS\n' > "$out/context-controls.expected.tsv"
  cmp "$out/context-controls.expected.tsv" "$out/context-controls.tsv"
  hold "$out/context-controls.tsv"; hold "$out/context-controls.expected.tsv"
  extra_env=(COLD560_PREPARE_HELPER_SHA256="$COLD560_PREPARE_HELPER_SHA256" COLD560_PREPARE_ROOT="$root"
    COLD560_PREPARE_CONTEXT=recovery474 COLD560_NONPROVIDER_GROWTH_BYTES="$COLD560_NONPROVIDER_GROWTH_BYTES"
    COLD560_JOB_DEADLINE_EPOCH="$COLD560_JOB_DEADLINE_EPOCH")
  start_owned provider-prepare 120 /usr/bin/bash "$preparer" build
  group_quiet "$active" "$active" "$out/logs/provider-prepare.group" || fail 'preparation left live/unknown members'
  complete_owned provider-prepare
  [[ $owned_code == 0 ]] || fail 'provider preparation failed'
  passed=true; exit
fi

[[ -f $base/prepare-complete && $(cat "$base/prepare-complete") == "$GITHUB_SHA" &&
   -f $job/proof/cold-strace-560/build-complete && $(cat "$job/proof/cold-strace-560/build-complete") == "$GITHUB_SHA" ]] || fail 'current private provider was not prepared; no stock fallback'
tracer=$root/bin/strace; fixture=$root/bin/approve-launch
owned_dir "$root" && owned_dir "$root/bin" && owned_dir "$job/proof/cold-strace-560" || fail 'provider roots are not private/owned'
[[ $(cat "$root/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB:$GITHUB_SHA" &&
   $(stat -c '%d:%i' "$root") == "$(cat "$job/proof/cold-strace-560/root.identity")" ]] || fail 'provider owner/inode differs'
provider_sha=$(sed -n 's/^sha256=//p' "$job/proof/cold-strace-560/provider.txt")
[[ $provider_sha =~ ^[0-9a-f]{64}$ && $(readlink -e "$tracer") == "$tracer" ]] || fail 'private provider identity'
printf '%s  %s\n' "$provider_sha" "$tracer" | sha256sum --check --strict --quiet -
hold "$tracer"; hold "$fixture"; hold "$job/proof/cold-strace-560/provider.txt"
if [[ $operation == after ]]; then
  phase=provider-after
  COLD560_VERIFY_PHASE=after-fast COLD560_STRACE=$tracer COLD560_PROVIDER_SHA256=$provider_sha /usr/bin/bash "$preparer" verify
  passed=true; exit
fi
load_recipes() {
  local diagnostic name recipe_fd
  diagnostic=$source/tools/ci/cold_tracer_diagnostic_560.sh
  printf '%s  %s\n' 8a7383587589d01404f7e6573db8fd1ad458c22fbc6ff2755298b5a4a35ddc4b "$diagnostic" | sha256sum --check --strict --quiet -
  hold "$diagnostic"
  exec {recipe_fd}<"$diagnostic"
  [[ $(stamp "/proc/$$/fd/$recipe_fd") == "${held_id[$diagnostic]}" ]] || fail 'recipe source FD differs'
  (set -o noclobber; : > "$out/closure.sh")
  for name in bind_elf inherited_controls run protocol_controls; do
    awk -v want="$name() {" '$0==want {if(seen++)exit 1;live=1} live {print} live && $0=="}" {live=0} END {if(seen!=1||live)exit 1}' \
      "/proc/$$/fd/$recipe_fd" >> "$out/closure.sh"
  done
  exec {recipe_fd}<&-
  load_definitions "$out/closure.sh" 6cc97850d050f6a18a24afd949eae3e9c7a8e189245f43ca384cb358f2335bf6
  hold "$out/closure.sh"
}
if [[ $operation == controls ]]; then
  deadline=$((SECONDS+460))
  load_recipes
  protocol_controls
  inherited_controls
  [[ $protocol_complete == true && $(wc -l < "$out/inheritance/controls.tsv") == 18 ]] || fail 'complete unchanged qualification required'
  COLD560_VERIFY_PHASE=after-cold COLD560_STRACE=$tracer COLD560_PROVIDER_SHA256=$provider_sha /usr/bin/bash "$preparer" verify
  passed=true; exit
fi
[[ -f $base/controls-complete && $(cat "$base/controls-complete") == "$GITHUB_SHA" ]] || fail '18 closure and 3 STOP/CONT protocol controls required'
sha256sum --check --strict "$job/proof/recovery-build/selected-before.sha256"
mkdir "$out/inheritance"
extra_env=(COLD560_LOG="$out/observer.log")
if [[ $operation == pty ]]; then
  deadline=$((SECONDS+100))
  load_recipes
  test_name=terminal::pty::tests::exited_leader_remains_reserved_until_owned_descendant_cleanup
  hold "${RECOVERY_FAMILY_CWP_TEST_BINARY:?}"
  inherited_command pty 30 pass none "$tracer" --seccomp-bpf -f -qq -s 32768 -xx -e trace=execve,execveat \
    "${launch_strip[@]}" -o "$out/pty.exec" "$RECOVERY_FAMILY_CWP_TEST_BINARY" --exact "$test_name" --test-threads=1 --nocapture
  grep -F "test $test_name ... " "$out/logs/pty.stdout" >/dev/null
  grep -F '1 passed; 0 failed' "$out/logs/pty.stdout" >/dev/null
  grep -F '"leader_reaped":true' "$out/logs/pty.stdout" >/dev/null
  ! grep -Eq '\?\?\?|= \?' "$out/pty.exec" || fail 'unclassified/unavailable PTY control trace'
  hold "$out/pty.exec"
  # A separate real controlling PTY exercises a stopping signal under the tracer,
  # not merely the outer launcher's STOP/CONT handshake. This is a probe, not a new approval protocol.
  cat > "$build/pty-stop-cont.c" <<'C'
#define _GNU_SOURCE
#include <errno.h>
#include <pty.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/wait.h>
#include <utmp.h>
#include <unistd.h>
static pid_t child = -1;
static void cleanup(void) {
  if (child > 0) {
    kill(child, SIGKILL);
    while (waitpid(child, NULL, 0) < 0 && errno == EINTR) {}
  }
}
static void require(int ok, const char *what) {
  if (!ok) { fprintf(stderr, "PTY474 failed: %s\n", what); exit(1); }
}
int main(int argc, char **argv) {
  if (argc == 2 && !strcmp(argv[1], "leaf")) {
    puts("PTY474_STOP_READY"); fflush(stdout);
    require(raise(SIGSTOP) == 0, "raise STOP");
    puts("PTY474_CONTINUED");
    return 0;
  }
  require(argc == 1 && atexit(cleanup) == 0 && signal(SIGCHLD, SIG_DFL) != SIG_ERR, "arguments/cleanup/SIGCHLD");
  int master, slave, status;
  require(openpty(&master, &slave, NULL, NULL, NULL) == 0, "openpty");
  pid_t parent = getpid();
  child = fork();
  require(child >= 0, "fork");
  if (child == 0) {
    close(master);
    if (prctl(PR_SET_PDEATHSIG, SIGKILL) || getppid() != parent || login_tty(slave)) _exit(125);
    execl(argv[0], argv[0], "leaf", (char *)NULL);
    _exit(126);
  }
  close(slave);
  pid_t result;
  do { result = waitpid(child, &status, WUNTRACED); } while (result < 0 && errno == EINTR);
  if (result == child && (WIFEXITED(status) || WIFSIGNALED(status))) { child = -1; require(0, "child exited before STOP"); }
  require(result == child && WIFSTOPPED(status) && WSTOPSIG(status) == SIGSTOP, "actual PTY group stop");
  require(kill(child, SIGCONT) == 0, "CONT owned child");
  do { result = waitpid(child, &status, 0); } while (result < 0 && errno == EINTR);
  require(result == child, "direct wait");
  pid_t reaped = child;
  child = -1;
  require(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child exit0 after CONT");
  char bytes[4096]; size_t used = 0;
  for (;;) {
    require(used < sizeof(bytes)-1, "bounded PTY output");
    ssize_t n = read(master, bytes+used, sizeof(bytes)-1-used);
    if (n > 0) { used += (size_t)n; continue; }
    if (n < 0 && errno == EINTR) continue;
    require(n == 0 || (n < 0 && errno == EIO), "PTY EOF after reap");
    break;
  }
  close(master); bytes[used] = 0;
  require(strstr(bytes, "PTY474_STOP_READY") != NULL && strstr(bytes, "PTY474_CONTINUED") != NULL, "actual PTY output before/after CONT");
  printf("RECOVERY474_PTY_STOP_CONT PASS child=%ld stopped=SIGSTOP continued=true reaped=0\n", (long)reaped);
  return 0;
}
C
  hold "$build/pty-stop-cont.c"
  # The unchanged shared owner/timeout applies to both compile and probe; no local compilation.
  start_owned pty-control-build 20 /usr/bin/gcc -std=c11 -Wall -Wextra -Werror -O2 "$build/pty-stop-cont.c" -lutil -o "$build/pty-stop-cont"
  group_quiet "$active" "$active" "$out/logs/pty-control-build.group" || fail 'PTY compile left live/unknown group'
  complete_owned pty-control-build
  [[ $owned_code == 0 ]] || fail 'PTY control build failed'
  bind_elf "$build/pty-stop-cont"
  extra_env=(COLD560_LOG="$out/pty-stop-cont.observer")
  inherited_command pty-stop-cont 20 pass none "$tracer" --seccomp-bpf -f -qq -s 32768 -xx -e trace=execve,execveat \
    "${launch_strip[@]}" -o "$out/pty-stop-cont.exec" "$build/pty-stop-cont"
  grep -Eq '^RECOVERY474_PTY_STOP_CONT PASS child=[1-9][0-9]* stopped=SIGSTOP continued=true reaped=0$' "$out/logs/pty-stop-cont.stdout"
  grep -F -- '--- SIGSTOP ' "$out/pty-stop-cont.exec" >/dev/null
  grep -F -- '--- SIGCONT ' "$out/pty-stop-cont.exec" >/dev/null
  ! grep -Eq '\?\?\?|= \?' "$out/pty-stop-cont.exec" || fail 'unclassified/unavailable STOP/CONT trace'
  [[ $(stat -c %s "$out/pty-stop-cont.observer") -le 65536 &&
     $(grep -Ec '^COLD560 finish .* io_error=0 overflow=0$' "$out/pty-stop-cont.observer") == 1 ]] || fail 'STOP/CONT observer health'
  hold "$out/pty-stop-cont.exec"; hold "$out/pty-stop-cont.observer"
else
  [[ -f $base/pty-complete && $(cat "$base/pty-complete") == "$GITHUB_SHA" ]] || fail 'real PTY HUP/KILL cleanup control required'
  [[ ${RECOVERY_FAMILY_FROZEN_EVIDENCE:-} == "$job/recovery-trace/native" ]] || fail 'complete trace native evidence root differs'
  deadline=$((SECONDS+900))
  hold "${RECOVERY_HARNESS:?}"
  inherited_command recovery 900 pass none "$tracer" --seccomp-bpf -ff -qq -s 32768 -xx -e trace=execve,execveat \
    "${launch_strip[@]}" -o "$job/recovery-trace/exec" "$RECOVERY_HARNESS" \
    --exact eight_native_recovery_controllers_same_elf_present_and_absent --test-threads=1 --nocapture
fi
[[ -f $out/observer.log && ! -L $out/observer.log && $(stat -c %s "$out/observer.log") -le 65536 &&
   $(grep -Ec '^COLD560 finish .* io_error=0 overflow=0$' "$out/observer.log") == 1 ]] || fail 'observer health'
hold "$out/observer.log"
passed=true
