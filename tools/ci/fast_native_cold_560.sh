#!/usr/bin/env bash
# #560 maintained Fast cold entry. GHA only; no Cargo warmup or SDK installation.
set -euo pipefail
set +m
umask 077
export LC_ALL=C PATH=/usr/bin:/bin
fail() { printf 'Fast cold 560: %s\n' "$*" >&2; exit 1; }
[[ $# == 1 && ( $1 == cold || $1 == after ) ]] || fail 'cold|after required'
operation=$1
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-}:${GITHUB_JOB:-} == true:bin9208/openpilot-rust:native &&
   ${RUNNER_NAME:-}:${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == bin-actions:self-hosted:Linux:X64 &&
   ${SOURCE_ALLOWED:-} == true && $(uname -s):$(uname -m) == Linux:x86_64 ]] || fail 'trusted registered Fast native required'
case ${GITHUB_EVENT_NAME:-} in push|workflow_dispatch|pull_request) ;; *) fail 'unsupported event';; esac
job=${CI_JOB_TEMP:?}; source=${DIAG_SOURCE:?}; owner=${DIAG_SOURCE_OWNER:?}; workspace=${DIAG_ACTIONS_WORKSPACE:?}
sdk=/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu
[[ $job == /tmp/openpilot-ci.?????? && $source == "$owner/source" && $PWD == "$source" && $GITHUB_WORKSPACE == "$source" &&
   ${HOME:-} == "$job/home" && ${RUNNER_TEMP:-} == "$job" && ${CARGO_HOME:-} == "$job/cargo-home" &&
   ${RUSTUP_HOME:-} == "$job/rustup-home" && ${CI_PREPARED_RUST_ROOT:-} == "$sdk" &&
   ${CI_PREPARED_RUST_MANIFEST:-} == "$job/proof/prepared-rust-files.before.sha256" &&
   ${CARGO_TARGET_DIR:-} == "$source/rust/target" && ${RUSTC:-} == "$sdk/bin/rustc" && ${RUSTDOC:-} == "$sdk/bin/rustdoc" &&
   ${CARGO_BUILD_JOBS:-}:${CARGO_INCREMENTAL:-}:${CARGO_PROFILE_DEV_DEBUG:-}:${CARGO_PROFILE_TEST_DEBUG:-} == 1:0:0:0 ]] || fail 'actual source/SDK/homes/profile differ'
case $owner in "$workspace/.diag296-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."??????) ;; *) fail 'source namespace differs';; esac
owned_dir() {
  [[ -d $1 && ! -L $1 && $(readlink -e "$1") == "$1" && $(stat -c '%u:%a' "$1") == "$(id -u):700" ]]
}
for path in "$job" "$job/proof" "$owner" "$source";do owned_dir "$path" || fail 'private owned root required';done
[[ $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(cat "$owner/.diag296-owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(stat -c '%d:%i' "$owner") == "$DIAG_SOURCE_IDENTITY" && $GITHUB_SHA =~ ^[0-9a-f]{40}$ &&
   $(git --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || fail 'owner/source binding differs'
unset GH_TOKEN GITHUB_TOKEN BASH_ENV ENV LD_PRELOAD LD_LIBRARY_PATH RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_TARGET RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER MAKEFLAGS MFLAGS CONFIG_SITE
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1
[[ ${FAST560_HELPER_SHA256:-} =~ ^[0-9a-f]{64}$ ]] || fail 'reviewed Fast helper SHA required'
printf '%s  %s\n' "$FAST560_HELPER_SHA256" "$source/tools/ci/fast_native_cold_560.sh" | sha256sum --check --strict --quiet -
out=$job/proof/fast-cold-560; build=$job/fast-cold-560; root=$job/cold-strace-560
preparer=$source/tools/ci/prepare-cold-strace-560
export COLD560_PREPARE_HELPER_SHA256=6c7d99e6f5c1ee7fc4929c105a8bbb80334a47ec75e7e990ca5ea47c3d81bd58
export COLD560_PREPARE_ROOT=$root COLD560_NONPROVIDER_GROWTH_BYTES=$((5632*1048576))
printf '%s  %s\n' "$COLD560_PREPARE_HELPER_SHA256" "$preparer" | sha256sum --check --strict --quiet -
phase=preflight; active= identity=; groups_ok=true; passed=false
mode=run-inherited; launch_label= launch_dir= launch_fault= launch_tracer= launch_parent=;launch_seen=false
owned_code= owned_waited=
declare -A held_fd=() held_id=() held_sha=() aliases=() alias_id=()
if [[ $operation == cold ]];then
  [[ ! -e $out && ! -L $out && ! -e $build && ! -L $build ]] || fail 'fresh Fast proof/work required'
  mkdir "$out" "$out/logs" "$out/inheritance" "$build" "$build/tmp"
else
  owned_dir "$out" && owned_dir "$build" || fail 'owned prior cold proof/work required'
fi
# Preserve even loader/preflight failures, before the full finalizer is available.
trap 'code=$?; if ((code!=0));then (set -o noclobber;printf "operation=%s\nphase=%s\noriginal_exit=%s\n" "$operation" "$phase" "$code" > "$out/first-failure");fi' EXIT
held_output=$out
if [[ $operation == after ]];then
  held_output=$out/final-held
  mkdir "$held_output"
fi
# Same FD loader as the diagnostic; shared function bytes and timers are unchanged.
launch_shared=$source/tools/ci/cold_tracer_launch_560.sh
launch_shared_sha=1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a
[[ -f $launch_shared && ! -L $launch_shared && $(readlink -e "$launch_shared") == "$launch_shared" ]] || fail 'canonical launch library required'
launch_shared_id=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$launch_shared")
exec {launch_shared_fd}<"$launch_shared"
launch_shared_pid=$BASHPID
[[ $(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "/proc/$launch_shared_pid/fd/$launch_shared_fd") == "$launch_shared_id" ]] || fail 'launch library FD identity'
printf '%s  %s\n' "$launch_shared_sha" "/proc/$launch_shared_pid/fd/$launch_shared_fd" | sha256sum --check --strict --quiet -
. "/proc/$launch_shared_pid/fd/$launch_shared_fd"
[[ $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$launch_shared") == "$launch_shared_id" &&
   $(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "/proc/$launch_shared_pid/fd/$launch_shared_fd") == "$launch_shared_id" ]] || fail 'launch library changed while loading'
exec {launch_shared_fd}<&-
COLD560_HELPER_SHA256=$launch_shared_sha
free_bytes() { df --output=avail -B1 "$1" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {n++;v=$1} END {if(NR!=2||n!=1)exit 1;print v}'; }
memory_available() {
  local value group current maximum remaining
  value=$(awk '$1=="MemAvailable:" && $2~/^[0-9]+$/ && $3=="kB" {n++;v=$2*1024} END {if(n!=1)exit 1;printf "%.0f\n",v}' /proc/meminfo) || return 1
  group=$(awk -F: '$1=="0" && $2=="" {n++;v=$3} END {if(n>1)exit 1;if(n==1)print v}' /proc/self/cgroup) || return 1
  if [[ -n $group ]];then
    [[ $group == /* && $group != *..* ]] || return 1
    group=$(readlink -e "/sys/fs/cgroup$group") || return 1
    while [[ $group == /sys/fs/cgroup || $group == /sys/fs/cgroup/* ]];do
      if [[ -e $group/memory.max || -e $group/memory.current ]];then
        maximum=$(cat "$group/memory.max") && current=$(cat "$group/memory.current") || return 1
        [[ $current =~ ^[0-9]+$ ]] || return 1
        if [[ $maximum != max ]];then
          [[ $maximum =~ ^[0-9]+$ ]] || return 1
          remaining=$((maximum-current));((remaining>=0)) || remaining=0
          ((value<=remaining)) || value=$remaining
        fi
      fi
      [[ $group != /sys/fs/cgroup ]] || break
      group=${group%/*}
    done
  fi
  printf '%s\n' "$value"
}
admit() {
  local n path cap allocated apparent dev available memory
  local -a roots=("$owner" "$job") caps=($((2*1024*1024*1024)) $((4*1024*1024*1024)))
  local -A need=() representative=()
  [[ ${COLD560_JOB_DEADLINE_EPOCH:-} =~ ^[0-9]+$ ]] || return 1
  ((EPOCHSECONDS<COLD560_JOB_DEADLINE_EPOCH)) || return 1
  for n in 0 1;do
    path=${roots[$n]};cap=${caps[$n]}
    allocated=$(du -sx --block-size=1 "$path") || return 1;allocated=${allocated%%$'\t'*}
    apparent=$(du -sx --apparent-size --block-size=1 "$path") || return 1;apparent=${apparent%%$'\t'*}
    [[ $allocated =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]] && ((allocated<=cap && apparent<=cap)) || return 1
    dev=$(stat -c %d "$path");representative[$dev]=$path;need[$dev]=$(( ${need[$dev]:-0}+cap-allocated ))
    printf '%s\t%s\t%s\t%s\t%s\n' "$phase" "$path" "$allocated" "$apparent" "$cap" >> "$out/admission.tsv" || return 1
  done
  for dev in "${!need[@]}";do
    available=$(free_bytes "${representative[$dev]}") || return 1
    ((available>=need[$dev])) || return 1
  done
  if [[ -d $root && -d $job/proof/cold-strace-560 ]];then
    for option in '' --apparent-size;do
      allocated=$(du -sx --block-size=1 $option "$root" "$job/proof/cold-strace-560" | awk '{n+=$1} END {printf "%.0f\n",n}') || return 1
      [[ $allocated =~ ^[0-9]+$ ]] && ((allocated<=512*1048576)) || return 1
    done
  fi
  memory=$(memory_available) || return 1
  printf '%s\t%s\t%s\n' "$phase" "$EPOCHSECONDS" "$memory" >> "$out/memory.tsv" || return 1
  # Initial compile forecast is 2GiB. While allocated, fail on actual severe pressure.
  if [[ $phase == preflight ]];then ((memory>=2*1024*1024*1024));else ((memory>=64*1024*1024));fi
}
seal_after() {
  local suffix=$1 status=0
  [[ $(git --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || status=1
  git ls-files -z > "$out/index.$suffix.bin" || status=1
  git ls-files --stage -z > "$out/stage.$suffix.bin" || status=1
  cmp "$job/proof/index-before.bin" "$out/index.$suffix.bin" || status=1
  cmp "$out/stage.before.bin" "$out/stage.$suffix.bin" || status=1
  tar --null --no-recursion --verbatim-files-from -T "$job/proof/index-before.bin" -cf - --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner |
    sha256sum > "$out/source.$suffix.sha256" || status=1
  cmp "$job/proof/source-before.sha256" "$out/source.$suffix.sha256" || status=1
  git diff --exit-code || status=1;git diff --cached --exit-code || status=1
  find "$sdk" -type f -print0 | sort -z | xargs -0 -r sha256sum > "$out/sdk.$suffix.sha256" || status=1
  find "$sdk" -printf '%D:%i\t%m:%U:%G\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$out/sdk.$suffix.roster" || status=1
  cmp "$CI_PREPARED_RUST_MANIFEST" "$out/sdk.$suffix.sha256" || status=1
  cmp "$job/proof/prepared-rust-roster.before.bin" "$out/sdk.$suffix.roster" || status=1
  return "$status"
}
provider_after() {
  local suffix=$1
  COLD560_VERIFY_PHASE=$suffix COLD560_STRACE=$root/bin/strace COLD560_PROVIDER_SHA256=$provider_sha \
    /usr/bin/bash "$preparer" verify
}
finish() {
  local original=$? final=0 suffix=$operation status leader
  trap - EXIT INT TERM HUP;set +e
  [[ $operation != cold ]] || suffix=after-cold
  [[ $operation != after ]] || suffix=after-fast
  if [[ -n $active ]];then
    leader=$active
    if kill_owned;then wait -f "$leader";else final=1;groups_ok=false;fi
    group_quiet "$leader" '' "$out/logs/$suffix.group-failure" || { final=1;groups_ok=false; }
    active= identity=
  fi
  out=$held_output verify_held || final=1
  seal_after "$suffix" || final=1
  if [[ -n ${provider_sha:-} ]];then provider_after "$suffix" || final=1;else final=1;fi
  admit || final=1
  [[ $original == 0 && $final == 0 && $groups_ok == true && $passed == true ]] || final=1
  (set -o noclobber;printf 'original_exit=%s\nfinalizer_exit=%s\nowned_groups_nonlive=%s\n' "$original" "$final" "$groups_ok" > "$out/$operation.status") || final=1
  if [[ $final == 0 ]];then
    (set -o noclobber;printf '%s\n' "$GITHUB_SHA" > "$out/$operation-complete") || final=1
  fi
  if [[ $original != 0 || $final != 0 ]];then
    (set -o noclobber;printf 'operation=%s\nphase=%s\noriginal_exit=%s\nfinalizer_exit=%s\n' "$operation" "$phase" "$original" "$final" > "$out/first-failure")
    exit 1
  fi
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
[[ ${COLD560_JOB_DEADLINE_EPOCH:-} =~ ^[0-9]+$ ]] || fail 'original first trusted step deadline required'
remaining=$((COLD560_JOB_DEADLINE_EPOCH-EPOCHSECONDS))
((remaining>0 && remaining<=420)) || fail 'original Fast seven-minute deadline expired/extended'
# The shared launcher keeps its original timeout/kill/drain constants. Reserve
# their existing 20s tail inside, rather than extending, the caller's deadline.
deadline=$((SECONDS+remaining-20))
out=$held_output hold "$launch_shared"
out=$held_output hold "$source/tools/ci/fast_native_cold_560.sh"
out=$held_output hold "$preparer"
[[ ${held_sha[$launch_shared]} == "$launch_shared_sha" && ${held_id[$launch_shared]} == "$launch_shared_id" &&
   ${held_sha[$source/tools/ci/fast_native_cold_560.sh]} == "$FAST560_HELPER_SHA256" &&
   ${held_sha[$preparer]} == "$COLD560_PREPARE_HELPER_SHA256" ]] || fail 'held implementation binding differs'
if [[ $operation == after ]];then
  if [[ -f $out/provider.sha256 && ! -L $out/provider.sha256 ]];then
    provider_sha=$(cat "$out/provider.sha256")
    [[ $provider_sha =~ ^[0-9a-f]{64}$ ]] || fail 'malformed cold provider digest'
  fi
  [[ ! -e $out/first-failure && ! -L $out/first-failure && $(cat "$out/cold-complete") == "$GITHUB_SHA" &&
     $(cat "$out/cold.status") == $'original_exit=0\nfinalizer_exit=0\nowned_groups_nonlive=true' ]] || fail 'cold failure remains sticky'
  while IFS=$'\t' read -r path metadata digest;do
    [[ $(readlink -e "$path") == "$path" && ! -L $path && $(stamp "$path") == "$metadata" ]] || fail 'cold held path changed'
    printf '%s  %s\n' "$digest" "$path" | sha256sum --check --strict --quiet -
  done < "$out/cold-held.tsv"
  printf '%s  %s\n' "$provider_sha" "$root/bin/strace" | sha256sum --check --strict --quiet -
  phase=after-fast;passed=true;exit
fi
git ls-files --stage -z > "$out/stage.before.bin"
admit || fail 'operation-specific 6GiB/initial 2GiB memory admission failed'
[[ ! -e $job/openpilot-disk-guard && ! -L $job/openpilot-disk-guard && ! -e $source/rust/target && ! -L $source/rust/target ]] || fail 'Cargo/bootstrap target was warmed before cold'
# Parent-owned bootstrap must also use one worker; do not disguise its source.
grep -Fxq '    CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 "${bootstrap_cargo[@]}" build \' "$source/rust/tools/check-disk-space" || fail 'parent must register bootstrap Cargo worker one'
base_env=(PATH="$sdk/bin:/usr/bin:/bin" HOME="$job/home" LC_ALL=C TZ=UTC TMPDIR="$build/tmp" RUNNER_TEMP="$job"
  CARGO_HOME="$job/cargo-home" RUSTUP_HOME="$job/rustup-home" RUSTUP_TOOLCHAIN=1.94.0 CARGO_TARGET_DIR="$source/rust/target"
  RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  GITHUB_ACTIONS=true GITHUB_REPOSITORY=bin9208/openpilot-rust RUNNER_ENVIRONMENT="$RUNNER_ENVIRONMENT" RUNNER_OS="$RUNNER_OS" RUNNER_ARCH="$RUNNER_ARCH"
  GITHUB_RUN_ID="$GITHUB_RUN_ID" GITHUB_RUN_ATTEMPT="$GITHUB_RUN_ATTEMPT" GITHUB_JOB="$GITHUB_JOB" GITHUB_SHA="$GITHUB_SHA" RUNNER_NAME="$RUNNER_NAME"
  GITHUB_EVENT_NAME="$GITHUB_EVENT_NAME" SOURCE_ALLOWED="$SOURCE_ALLOWED" CI_JOB_TEMP="$job" GITHUB_WORKSPACE="$source"
  DIAG_SOURCE="$source" DIAG_SOURCE_OWNER="$owner" DIAG_SOURCE_IDENTITY="$DIAG_SOURCE_IDENTITY" DIAG_ACTIONS_WORKSPACE="$workspace"
  CI_PREPARED_RUST_ROOT="$sdk" CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST"
  CARGO_TERM_COLOR=never GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1)
extra_env=(COLD560_PREPARE_HELPER_SHA256="$COLD560_PREPARE_HELPER_SHA256" COLD560_PREPARE_ROOT="$root"
  COLD560_NONPROVIDER_GROWTH_BYTES="$COLD560_NONPROVIDER_GROWTH_BYTES" COLD560_JOB_DEADLINE_EPOCH="$COLD560_JOB_DEADLINE_EPOCH")
start_owned provider-prepare 120 /usr/bin/bash "$preparer" build
group_quiet "$active" "$active" "$out/logs/provider-prepare.group-before-release" || fail 'preparation left live/unknown group'
complete_owned provider-prepare
[[ $owned_code == 0 ]] || fail 'private provider preparation failed'
[[ $(cat "$job/proof/cold-strace-560/build-complete") == "$GITHUB_SHA" ]] || fail 'provider preparation incomplete'
tracer=$root/bin/strace;fixture=$root/bin/approve-launch
for path in "$tracer" "$fixture" /usr/bin/timeout "$source/.github/workflows/fast-checks.yml";do hold "$path";done
provider_sha=${held_sha[$tracer]}
printf '%s\n' "$provider_sha" > "$out/provider.sha256"
launch_strip=(-E COLD560_LOG -E COLD560_FILTER_INHERIT -E COLD560_LAUNCH_DIR -E COLD560_SOURCE_SHA -E COLD560_HELPER_SHA -E COLD560_PROVIDER_SHA)
extra_env=(COLD560_LOG="$out/observer.log" OPENPILOT_CI_PREPARED_RUST_ROOT="$sdk" OPENPILOT_CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST")
[[ ! -e $job/openpilot-disk-guard && ! -L $job/openpilot-disk-guard ]] || fail 'provider preparation warmed cold bootstrap'
inherited_command cold 420 pass none "$tracer" --debug --seccomp-bpf -f -qq -s 65535 -e trace=execve,execveat "${launch_strip[@]}" \
  -o "$job/proof/prepared-cold-bootstrap.exec" "$source/rust/tools/check-disk-space" --path "$source" --growth-mib 2048
extra_env=()
[[ $(stat -c '%u:%a:%h' "$out/observer.log") == "$(id -u):600:1" && $(stat -c %s "$out/observer.log") -le 65536 ]]
awk '
  BEGIN {events=0;anomalies=0}
  NR==1 {if($0!="COLD560 start schema=1")exit 1;next}
  $1=="COLD560"&&$2=="event" {
    events++;
    if(footer||NF!=13||$3!=("seq=" events)||$4!~/^stage=(get_syscall_info|get_scno|get_syscall_args|decode_failure)$/||$5!~/^tracer=[1-9][0-9]*$/||$6!~/^tracee=[1-9][0-9]*$/||$7!~/^rc=-?[0-9]+$/||$8!~/^errno=[0-9]+$/||$9!~/^op=-?[0-9]+$/||$10!~/^flags=(0|0x[0-9a-f]+)$/||$11!~/^entering=[01]$/||$12!~/^wait=(0|0x[0-9a-f]+)$/||$13!~/^event=[0-9]+$/)exit 1;
    pid=substr($5,8);if(owner&&owner!=pid)exit 1;owner=pid;
    if($4=="stage=get_syscall_info") {anomalies++;if(substr($7,4)+0<1&&$9!="op=-1")exit 1}next
  }
  $1=="COLD560"&&$2=="finish" {
    if(footer||NF!=13||$3!~/^pid=[1-9][0-9]*$/||$4!~/^seen=[0-9]+$/||$5!~/^written=[0-9]+$/||$6!~/^sci=[0-9]+$/||$7!~/^entry=[0-9]+$/||$8!~/^exit=[0-9]+$/||$9!~/^seccomp=[0-9]+$/||$10!~/^abnormal=[0-9]+$/||$11!~/^bytes=[0-9]+$/||$12!="io_error=0"||$13!="overflow=0")exit 1;
    if(owner&&$3!=("pid=" owner))exit 1;
    seen=substr($4,6)+0;written=substr($5,9)+0;sci=substr($6,5)+0;
    normal=substr($7,7)+substr($8,6)+substr($9,9);abnormal=substr($10,10)+0;
    if(sci<1||sci!=normal+abnormal||seen!=normal+events||written!=events||abnormal!=anomalies)exit 1;
    footer=NR;next
  }
  {exit 1}
  END {if(!footer||footer!=NR)exit 1}
' "$out/observer.log"
hold "$out/observer.log";hold "$job/proof/prepared-cold-bootstrap.exec"
cp "$out/held-before.tsv" "$out/cold-held.tsv"
passed=true
