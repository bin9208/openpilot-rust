#!/usr/bin/env bash
# #559 격리 후보: source-only 작성. 로컬 실행 금지, 기능/RED/GREEN은 Actions 전용이다.
set -euo pipefail
set +m
umask 077
export LC_ALL=C PATH=/usr/bin:/bin
fail() { printf 'UI recording 559: %s\n' "$*" >&2; exit 1; }
[[ $# == 1 && ( $1 == admit || $1 == run || $1 == final-source ) ]] || fail 'admit|run|final-source required'
mode=$1
[[ ${GITHUB_ACTIONS:-} == true && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust &&
   ${GITHUB_EVENT_NAME:-} == push && ${GITHUB_REF:-} == refs/heads/codex/feat-523-python-rust &&
   ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:X64 &&
   $(uname -s) == Linux && $(uname -m) == x86_64 ]] || fail 'trusted hosted source required'
source=${DIAG_SOURCE:?}; owner=${DIAG_SOURCE_OWNER:?}; workspace=${DIAG_ACTIONS_WORKSPACE:?}; job=${CI_JOB_TEMP:?}
[[ $job == /tmp/openpilot-ci.?????? && $PWD == "$source" && ${GITHUB_WORKSPACE:-} == "$source" &&
   $source == "$owner/source" && ${HOME:-} == "$job/home" && ${RUSTUP_HOME:-} == "$job/rustup-home" &&
   ${CARGO_HOME:-} == "$job/build/cargo-home" && ${CARGO_TARGET_DIR:-} == "$job/build/candidate-target" ]] || fail 'private layout differs'
case $owner in "$workspace/.ui559-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."??????) ;; *) fail 'owner namespace differs';; esac
for path in "$owner" "$source" "$job" "$job/proof"; do
  [[ -d $path && ! -L $path && $(readlink -e "$path") == "$path" && $(stat -c %u "$path") == "$(id -u)" ]] || fail 'owned canonical root required'
done
[[ $(stat -c %a "$job") == 700 && $(stat -c '%d:%i' "$owner") == "$DIAG_SOURCE_IDENTITY" &&
   $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(cat "$owner/.diag296-owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $GITHUB_SHA =~ ^[0-9a-f]{40}$ && $(git --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || fail 'owner/source binding differs'
[[ ${UI559_HELPER_SHA256:-} =~ ^[0-9a-f]{64}$ ]]
printf '%s  %s\n' "$UI559_HELPER_SHA256" "$source/tools/ci/ui_recording_559.sh" | sha256sum --check --strict --quiet -
unset GH_TOKEN GITHUB_TOKEN BASH_ENV ENV LD_PRELOAD LD_LIBRARY_PATH RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_TARGET RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
export GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
proof=$job/proof/ui559
build=$job/build
tools=$job/ui559-tools
base_owner=$job/ui559-baseline
baseline=$base_owner/source
sdk=$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu
gib=$((1024*1024*1024)); reserve=$((512*1024*1024)); scan_role=main
free_bytes() {
  df --output=avail -B1 "$1" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {n++;v=$1} END {if(NR!=2 || n!=1) exit 1;print v}'
}
# mutable build/tmp/기존 target 및 baseline .git의 정확한 ENOENT만 최대3회 재표본한다.
usage() {
  local root=$1 cap=$2 code values bytes apparent line relative before attempt prefix suffix path
  local errors=$job/proof/ui559-scan-$scan_role.stderr
  [[ -e $root || -L $root ]] || { printf '0 0\n'; return; }
  [[ -d $root && ! -L $root && $(readlink -e "$root") == "$root" ]] || return 1
  before=$(stat -c '%d:%i' "$root") || return 1
  for attempt in 1 2 3; do
    code=0
    values=$(
      set +e
      find -P "$root" -ignore_readdir_race -printf '%b %s\n' 2> "$errors" |
        awk '{b+=$1*512;s+=$2} END {printf "%.0f %.0f\n",b,s}'
      codes=("${PIPESTATUS[@]}")
      ((codes[1]==0)) || exit 125
      exit "${codes[0]}"
    ) || code=$?
    [[ $(stat -c '%d:%i' "$root") == "$before" ]] || return 1
    if [[ $code == 0 || $code == 1 ]]; then
      read -r bytes apparent <<< "$values"
      [[ $bytes =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]] && ((bytes<=cap && apparent<=cap)) || return 1
    fi
    if [[ $code == 0 && ! -s $errors ]]; then printf '%s\n' "$values"; return; fi
    [[ $code == 1 && -s $errors ]] || return 1
    case $root in
      "$build") prefix="find: '$build/";;
      "$base_owner") prefix="find: '$baseline/.git/";;
      *) return 1;;
    esac
    suffix="': No such file or directory"
    while IFS= read -r line || [[ -n $line ]]; do
      [[ $line == "$prefix"*"$suffix" ]] || return 1
      relative=${line#"$prefix"}; relative=${relative%"$suffix"}
      [[ $relative =~ ^[A-Za-z0-9_./+-]+$ ]] || return 1
      case "/$relative/" in *'/../'*|*'/./'*|*'//'*) return 1;; esac
      path=${prefix#"find: '"}$relative
      [[ ! -e $path && ! -L $path ]] || return 1
    done < "$errors"
    cat "$errors" >> "$job/proof/ui559-budget-races.log" || return 1
  done
  return 1
}
admit() {
  local n path cap used apparent device available sdk_used=0 sdk_apparent=0
  local -A needs=() representatives=()
  local -a paths=("$owner" "$base_owner" "$build" "$tools" "$job/proof")
  local -a caps=($((2*gib)) $((2*gib)) $((8*gib)) $((2*gib)) "$gib")
  for path in "$base_owner" "$build" "$tools" "$job/proof" "$job/rustup-home" "$job/cargo-home" "$job/rust-bootstrap"; do
    [[ ! -e $path ]] || [[ $(stat -c %d "$path") == "$(stat -c %d "$job")" ]] || return 1
  done
  for n in "${!paths[@]}"; do
    path=${paths[$n]};cap=${caps[$n]}
    read -r used apparent <<< "$(usage "$path" "$cap")" || return 1
    [[ $used =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]] || return 1
    if [[ $path == "$build" && ${phase:-} == clippy ]]; then
      [[ ${clippy_base:-} =~ ^[0-9]+$ && ${clippy_apparent_base:-} =~ ^[0-9]+$ ]] || return 1
      ((used-clippy_base<=512*1024*1024 && apparent-clippy_apparent_base<=512*1024*1024)) || return 1
    fi
    device=$(stat -c %d "$( [[ -d $path ]] && printf '%s' "$path" || printf '%s' "$job" )") || return 1
    representatives[$device]=$( [[ -d $path ]] && printf '%s' "$path" || printf '%s' "$job" )
    needs[$device]=$(( ${needs[$device]:-0}+cap-used ))
  done
  for path in "$job/rustup-home" "$job/cargo-home" "$job/rust-bootstrap"; do
    read -r used apparent <<< "$(usage "$path" $((3*gib)))" || return 1
    sdk_used=$((sdk_used+used));sdk_apparent=$((sdk_apparent+apparent))
    ((sdk_used<=3*gib && sdk_apparent<=3*gib)) || return 1
  done
  device=$(stat -c %d "$job");representatives[$device]=$job
  needs[$device]=$(( ${needs[$device]:-0}+3*gib-sdk_used ))
  if [[ ! -e $job/proof/ui559-packages.done ]]; then
    device=$(stat -c %d /);representatives[$device]=/
    needs[$device]=$(( ${needs[$device]:-0}+2*gib ))
  else
    [[ ! -L $job/proof/ui559-packages.done && $(cat "$job/proof/ui559-packages.done") == "$GITHUB_SHA" ]] || return 1
  fi
  for device in "${!needs[@]}"; do
    available=$(free_bytes "${representatives[$device]}") || return 1
    printf '%s\t%s\t%s\t%s\n' "$EPOCHREALTIME" "$device" "$available" "$((needs[$device]+reserve))" >> "$job/proof/ui559-admission.tsv" || return 1
    ((available>=needs[$device]+reserve)) || return 1
  done
}
source_final() {
  local status=0
  git ls-files -z > "$job/proof/index-after.bin" || status=1
  cmp "$job/proof/index-before.bin" "$job/proof/index-after.bin" || status=1
  tar --null --no-recursion --verbatim-files-from -T "$job/proof/index-before.bin" -cf - --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner |
    sha256sum > "$job/proof/source-after.sha256" || status=1
  cmp "$job/proof/source-before.sha256" "$job/proof/source-after.sha256" || status=1
  git diff --exit-code || status=1
  git diff --cached --exit-code || status=1
  admit || status=1
  return "$status"
}
if [[ $mode == admit ]]; then admit; exit; fi
if [[ $mode == final-source ]]; then source_final; exit; fi
[[ ${CI_PREPARED_RUST_ROOT:-} == "$sdk" && -f ${CI_PREPARED_RUST_MANIFEST:?} ]] || fail 'prepared SDK missing'
[[ ! -e $proof && ! -L $proof && ! -e $build && ! -L $build && ! -e $tools && ! -L $tools && ! -e $base_owner && ! -L $base_owner ]]
mkdir "$proof" "$proof/logs" "$proof/trace" "$proof/elfs" "$proof/failures" "$build" "$build/cargo-home" "$build/tmp" "$build/home" "$build/cache" "$tools" "$base_owner"
printf '%s\n' "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" > "$base_owner/owner"
printf '%s\n' "$(stat -c '%d:%i' "$base_owner")" > "$proof/baseline-owner.identity"
phase=preflight; passed=false; established=false; baseline_sealed=false; red=false; green=false; suite=false; graphics=false; clippy=false
active= active_identity= watcher= stop=
deadline=$((SECONDS+1200))
ulimit -Sn 4096
ulimit -c 0
declare -A held_fd=() held_id=() held_sha=() aliases=() alias_id=() elf_seen=()
declare -a tree_paths=() tree_names=()
idfile() { stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$1"; }
hold() {
  local path=$1 canonical before fd digest
  canonical=$(readlink -e -- "$path") || return 1
  [[ -f $canonical && ! -L $canonical && $(stat -c %s "$canonical") -le 536870912 ]] || return 1
  before=$(idfile "$canonical") || return 1
  if [[ -v aliases[$path] ]]; then
    [[ ${aliases[$path]} == "$canonical" && ${alias_id[$path]} == "$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path")" ]] || return 1
  else
    aliases[$path]=$canonical
    alias_id[$path]=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") || return 1
  fi
  if [[ -v held_fd[$canonical] ]]; then
    [[ ${held_id[$canonical]} == "$before" ]] || return 1
    return
  fi
  exec {fd}<"$canonical" || return 1
  [[ $(idfile "/proc/$$/fd/$fd") == "$before" ]] || return 1
  digest=$(sha256sum "/proc/$$/fd/$fd") || return 1; digest=${digest%% *}
  [[ $(idfile "$canonical") == "$before" && $(idfile "/proc/$$/fd/$fd") == "$before" ]] || return 1
  held_fd[$canonical]=$fd; held_id[$canonical]=$before; held_sha[$canonical]=$digest
  printf '%s\t%s\t%s\n' "$canonical" "$before" "$digest" >> "$proof/held-before.tsv"
}
verify_held() {
  local path fd hash status=0
  for path in "${!aliases[@]}"; do
    [[ $(readlink -e "$path") == "${aliases[$path]}" && $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") == "${alias_id[$path]}" ]] || status=1
  done
  for path in "${!held_fd[@]}"; do
    fd=${held_fd[$path]}
    [[ $(idfile "$path") == "${held_id[$path]}" && $(idfile "/proc/$$/fd/$fd") == "${held_id[$path]}" ]] || { status=1; continue; }
    hash=$(sha256sum "/proc/$$/fd/$fd") || { status=1; continue; }; hash=${hash%% *}
    [[ $hash == "${held_sha[$path]}" && $(idfile "$path") == "${held_id[$path]}" ]] || status=1
    printf '%s\t%s\t%s\n' "$path" "${held_id[$path]}" "$hash" >> "$proof/held-after.tsv" || status=1
  done
  return "$status"
}
tree() {
  local root=$1 out=$2
  find -P "$root" -path "$source/.git" -prune -o -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$out.roster" || return 1
  find -P "$root" -path "$source/.git" -prune -o -type f -print0 | sort -z | xargs -0 -r sha256sum > "$out.sha256" || return 1
  find -P "$root" -path "$source/.git" -prune -o -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$out.stable" || return 1
  cmp "$out.roster" "$out.stable"
}
add_tree() {
  [[ -d $2 && ! -L $2 && $(readlink -e "$2") == "$2" ]] || return 1
  tree_names+=("$1"); tree_paths+=("$2")
  tree "$2" "$proof/$1.before"
}
pid_identity() {
  local line rest
  local -a fields
  IFS= read -r line < "/proc/$1/stat" || return 1
  [[ ${line%% *} == "$1" ]] || return 1
  rest=${line##*) }; read -r -a fields <<< "$rest"
  [[ ${#fields[@]} -ge 20 && ${fields[0]} != Z && ${fields[0]} != X && ${fields[1]} == "$$" && ${fields[2]} == "$1" && ${fields[3]} == "$1" ]] || return 1
  printf '%s\n' "${fields[19]}"
}
kill_owned() {
  [[ -n $active && -n $active_identity && $(pid_identity "$active") == "$active_identity" ]] || return 1
  kill -KILL -- "-$active"
}
group_empty() {
  local target=$1 entry line rest
  local -a fields
  for entry in /proc/[0-9]*/stat; do
    if ! IFS= read -r line < "$entry"; then
      [[ ! -e $entry ]] && continue
      return 1
    fi
    rest=${line##*) }; read -r -a fields <<< "$rest"
    [[ ${#fields[@]} -ge 20 && ${fields[2]} =~ ^[0-9]+$ ]] || return 1
    [[ ${fields[2]} != "$target" ]] || return 1
  done
  printf '%s\tabsent-after-direct-wait\n' "$target" >> "$proof/group-final.tsv"
}
bind_elf() {
  local path=$1 canonical line resolved interp code=0
  hold "$path" || return 1
  canonical=$(readlink -e "$path") || return 1
  [[ -v elf_seen[$canonical] ]] && return 0
  elf_seen[$canonical]=true
  # shell script provider도 본문/실제 interpreter를 별도로 봉인한다.
  if [[ $(head -c 4 "$canonical" | od -An -tx1 | tr -d ' \n') != 7f454c46 ]]; then
    IFS= read -r line < "$canonical" || return 1
    # actual which.debianutils의 동일 /bin/sh 표기이며 본문/인터프리터 hold는 유지한다.
    case $line in '#!/bin/sh'|'#!/usr/bin/sh'|'#! /bin/sh') bind_elf /bin/sh;;
      '#!/bin/bash'|'#!/usr/bin/bash') bind_elf /bin/bash;; *) return 1;; esac
    return
  fi
  readelf -h "$canonical" > "$proof/elf.header" || return 1
  grep -Eq 'Class: +ELF64$' "$proof/elf.header" && grep -Eq 'Machine: +Advanced Micro Devices X86-64$' "$proof/elf.header" &&
    grep -Eq 'Type: +(DYN|EXEC) ' "$proof/elf.header" || return 1
  readelf -l "$canonical" > "$proof/elf.program" || return 1
  readelf -d "$canonical" > "$proof/elf.dynamic" || return 1
  interp=$(sed -n 's/.*Requesting program interpreter: \([^]]*\)\].*/\1/p' "$proof/elf.program") || return 1
  [[ -z $interp || $interp == /lib64/ld-linux-x86-64.so.2 ]] || return 1
  if [[ -z $interp ]] && ! grep -q '(NEEDED)' "$proof/elf.dynamic"; then
    grep -q 'LOAD' "$proof/elf.program" || return 1
    printf '%s\tstatic-empty-DSO\n' "$canonical" >> "$proof/providers.tsv"
    return
  fi
  # 검증한 x64 ELF/표준 interpreter의 현재 loader 해석이다. 별도 runtime dlopen은 과장하지 않는다.
  /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout 30s /usr/bin/ldd "$canonical" > "$proof/elf.ldd" 2> "$proof/elf.ldd.stderr" || code=$?
  [[ $code == 0 && ! -s $proof/elf.ldd.stderr ]] || return 1
  while IFS= read -r line; do
    [[ $line =~ ^[[:space:]]*linux-vdso.so.1[[:space:]]+\(0x[0-9a-f]+\)$ ]] && continue
    if [[ $line =~ ^[[:space:]]*[^[:space:]]+[[:space:]]+=\>[[:space:]]+(/[^[:space:]]+)[[:space:]]+\(0x[0-9a-f]+\)$ ]]; then resolved=${BASH_REMATCH[1]}
    elif [[ $line =~ ^[[:space:]]*(/[^[:space:]]+)[[:space:]]+\(0x[0-9a-f]+\)$ ]]; then resolved=${BASH_REMATCH[1]}
    else return 1; fi
    hold "$resolved" || return 1
    printf '%s\t%s\n' "$canonical" "$resolved" >> "$proof/providers.tsv" || return 1
  done < "$proof/elf.ldd"
}
trace_nonlive() {
  local directory=$1 path pid row rest
  local -a files fields
  shopt -s nullglob
  files=("$directory"/trace.*)
  ((${#files[@]}>0)) || return 1
  for path in "${files[@]}"; do
    pid=${path##*.}; [[ $pid =~ ^[1-9][0-9]*$ && -f $path && ! -L $path ]] || return 1
    if [[ -d /proc/$pid ]]; then
      if ! IFS= read -r row < "/proc/$pid/stat"; then [[ ! -d /proc/$pid ]] && continue; return 1; fi
      rest=${row##*) };read -r -a fields <<< "$rest"
      [[ ${fields[0]} == Z || ${fields[0]} == X ]] || return 1
    fi
  done
}
baseline_tree() {
  local saved_source=$source result=0
  source=$baseline
  tree "$baseline" "$1" || result=$?
  source=$saved_source
  return "$result"
}
finish() {
  local original=$? status=0 n sealed=false nonlive=true
  trap - EXIT INT TERM HUP
  set +e
  if [[ -n $active ]]; then
    kill_owned || { status=1; nonlive=false; }
    wait "$active"
    group_empty "$active" || { status=1; nonlive=false; }
    active=
  fi
  if [[ -n $watcher ]]; then : > "$stop"; wait "$watcher" || status=1; watcher=; fi
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr" || status=1
  for n in "${!tree_paths[@]}"; do
    tree "${tree_paths[$n]}" "$proof/${tree_names[$n]}.after" &&
      cmp "$proof/${tree_names[$n]}.before.roster" "$proof/${tree_names[$n]}.after.roster" &&
      cmp "$proof/${tree_names[$n]}.before.sha256" "$proof/${tree_names[$n]}.after.sha256" || status=1
  done
  if $baseline_sealed; then
    baseline_tree "$proof/baseline.final" &&
      cmp "$proof/baseline.before.roster" "$proof/baseline.final.roster" &&
      cmp "$proof/baseline.before.sha256" "$proof/baseline.final.sha256" || status=1
    [[ $(git -C "$baseline" --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || status=1
    git -C "$baseline" --no-optional-locks ls-files -z > "$proof/baseline-index.after" || status=1
    cmp "$proof/baseline-index.before" "$proof/baseline-index.after" || status=1
  fi
  for path in "$proof/trace"/*; do
    [[ ! -d $path ]] || trace_nonlive "$path" || { status=1; nonlive=false; }
  done
  source_final || status=1
  [[ ! -e $job/proof/ui559-resource-stop ]] || status=1
  [[ $established == true && $status == 0 ]] && sealed=true
  jq -n --arg source "$GITHUB_SHA" --arg phase "$phase" --argjson code "$original" --argjson final "$status" \
    --argjson seals "$sealed" --argjson nonlive "$nonlive" --argjson red "$red" --argjson green "$green" \
    --argjson suite "$suite" --argjson graphics "$graphics" --argjson clippy "$clippy" \
    '{schema:1,source:$source,phase:$phase,command_exit:$code,finalizer_exit:$final,immutable_seals:$seals,
      owned_processes_nonlive:$nonlive,baseline_exact_red:$red,candidate_exact_green:$green,recording9:$suite,
      original_five_modes:$graphics,strict_clippy:$clippy,pass:($code==0 and $final==0 and $seals and $nonlive and $red and $green and $suite and $graphics and $clippy),
      historical_success_inherited:false,original_expected_regenerated:false,actual_root_confirmed:false,
      all_adopted_children_reaped:false,closed_fd:false,closed_dso:false,retirement_credit:0,
      cleanup_scope:"current owned leader waited; process tracees non-live; adopted wait not claimed",measured_peak_guarantee:false}' \
      > "$proof/outcome.json" || status=1
  printf 'immutable_seals=%s\nowned_processes_nonlive=%s\n' "$sealed" "$nonlive" >> "$GITHUB_OUTPUT" || status=1
  [[ $original == 0 && $status == 0 && $passed == true ]] || exit 1
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
build_env=(HOME="$build/home" XDG_CACHE_HOME="$build/cache" PATH="$sdk/bin:/usr/bin:/bin" TMPDIR="$build/tmp" LC_ALL=C TZ=UTC CXX=clang++
  CARGO_HOME="$build/cargo-home" RUSTUP_HOME="$job/rustup-home" RUSTUP_TOOLCHAIN=1.94.0
  RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TERM_COLOR=never
  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 GIT_LFS_SKIP_SMUDGE=1)
run() {
  local label=$1 seconds=$2 expected=$3 code=0 watch_code=0 leader ready go count remaining started=$EPOCHREALTIME
  shift 3
  phase=$label;admit
  [[ ! -e $job/proof/ui559-resource-stop ]]
  remaining=$((deadline-SECONDS));((remaining>0)) || fail '20-minute execution deadline'
  ((seconds<=remaining)) || seconds=$remaining
  mkdir "$proof/trace/$label"
  ready=$proof/logs/$label.ready; go=$proof/logs/$label.go; stop=$proof/logs/$label.stop
  printf '%s\0' "$label" "$@" >> "$proof/commands.nul"
  /usr/bin/setsid /usr/bin/bash --noprofile --norc -c '
    set -eu; ulimit -c 0; ulimit -f 524288
    ready=$1;go=$2;shift 2;printf "%s\n" "$$" > "$ready"
    for ((n=0;n<300;n++)); do [[ ! -e $go ]] || break;sleep 0.01;done
    [[ -e $go ]] || exit 125
    exec "$@"
  ' ui559-owned "$ready" "$go" /usr/bin/env -i "${build_env[@]}" "${extra_env[@]}" \
    /usr/bin/timeout --foreground --signal=TERM --kill-after=15s "${seconds}s" \
    /usr/bin/strace --kill-on-exit -ff -qq -s 65535 -e trace=process -o "$proof/trace/$label/trace" "$@" \
    > "$proof/logs/$label.stdout" 2> "$proof/logs/$label.stderr" < /dev/null &
  leader=$!;active=$leader;active_identity=
  for ((count=0;count<300;count++)); do
    if [[ -f $ready && $(cat "$ready") == "$leader" ]]; then active_identity=$(pid_identity "$leader") && break;fi
    sleep 0.01
  done
  [[ -n $active_identity ]] || fail 'owned PID/starttime handshake'
  : > "$go"
  (
    scan_role=watcher
    while [[ ! -e $stop ]]; do
      if [[ $(pid_identity "$leader") != "$active_identity" ]]; then
        # 정상 종료/직접 wait 경합만 종료로 인정한다. 살아 있는 unknown은 감시 성공이 아니다.
        [[ -d /proc/$leader ]] || break
        row=
        if IFS= read -r row < "/proc/$leader/stat"; then
          rest=${row##*) };read -r -a ended <<< "$rest"
          if [[ ${#ended[@]} -ge 20 && ( ${ended[0]} == Z || ${ended[0]} == X ) &&
                ${ended[1]} == "$$" && ${ended[2]} == "$leader" && ${ended[3]} == "$leader" && ${ended[19]} == "$active_identity" ]]; then break;fi
        elif [[ ! -d /proc/$leader ]];then break;fi
        printf '%s ownership-unknown\n' "$label" > "$job/proof/ui559-resource-stop"
        exit 1
      fi
      if ! admit;then printf '%s\n' "$label" > "$job/proof/ui559-resource-stop";kill_owned || true;exit 1;fi
      sleep 0.5
    done
  ) &
  watcher=$!
  wait "$leader" || code=$?
  : > "$stop";wait "$watcher" || watch_code=$?
  active= active_identity= watcher=
  group_empty "$leader" && trace_nonlive "$proof/trace/$label" || fail 'owned child still live or unknown after wait'
  printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$code" "$watch_code" "$started" "$EPOCHREALTIME" >> "$proof/exit-codes.tsv"
  ((code==expected && watch_code==0)) || fail "$label exit $code, expected $expected; watcher $watch_code"
  admit
}
artifact() {
  local log=$1 root=$2 name=$3 kind=$4 src=$5 tested=$6 features=$7 crate=${8:-dev-tools}
  case $crate in dev-tools|disk-space) ;; *) return 1;; esac
  jq -ser --arg package "path+file://$root/rust/crates/$crate#openpilot-$crate@0.1.0" \
    --arg manifest "$root/rust/crates/$crate/Cargo.toml" --arg name "$name" --arg kind "$kind" \
    --arg src "$root/$src" --argjson tested "$tested" --argjson features "$features" '
    if length<1 or .[-1]!={reason:"build-finished",success:true} or
      ([.[]|select(.reason=="build-finished")]|length)!=1 then error("Cargo EOF") else . end |
    [.[]|select(.reason=="compiler-artifact" and .package_id==$package and .manifest_path==$manifest and
      .target.name==$name and .target.kind==[$kind] and .target.src_path==$src and .profile.test==$tested and
      (.features|sort)==$features and .profile.opt_level=="0" and .executable!=null)] |
    if length==1 then .[0].executable else error("exact current Cargo target required") end' "$log"
}
copy_elf() {
  local origin=$1 name=$2 root=$3
  [[ $origin == "$build/$root-target/"* && $(readlink -e "$origin") == "$origin" && -x $origin && ! -L $origin ]]
  cp --reflink=never -- "$origin" "$proof/elfs/$name"
  chmod 500 "$proof/elfs/$name"
  cmp "$origin" "$proof/elfs/$name"
  bind_elf "$proof/elfs/$name"
}
ok_test() {
  local label=$1 count=$2
  [[ $(grep -Ec "^test result: ok\\. $count passed; 0 failed; 0 ignored;" "$proof/logs/$label.stdout") == 1 ]]
}
extra_env=()
for tool in bash sh env git find sort xargs sha256sum stat readlink readelf sed awk grep head od tr cat dd true cmp df du jq strace timeout setsid sleep curl clang++ g++ cc as ar ranlib ld nm pkg-config capnp tar gzip cmake make gmake xauth Xvfb xvfb-run ffmpeg ffprobe cp chmod mkdir id dirname basename cut rm ldd mktemp touch ps tail wc uname which getopt; do
  # command -v true는 builtin 이름을 반환한다. 봉인할 외부 PATH provider를 선택한다.
  path=$(type -P "$tool") || fail "external provider missing: $tool"
  [[ $path == /* ]] || fail "external provider path is not absolute: $tool"
  bind_elf "$path" || fail "provider admission failed: $tool ($path)"
done
for path in /bin/cat /bin/dd /bin/true /bin/sh /bin/bash;do bind_elf "$path";done
for tool in cargo rustc rustdoc clippy-driver; do bind_elf "$sdk/bin/$tool";done
hold /etc/ld.so.cache
hold "$source/tools/ci/ui_recording_559.sh"
overlay=$source/tools/ci/fixtures/ui-recording-559-baseline.patch
hold "$overlay"
printf '%s  %s\n' 0d516594bf5dc43ee9c000b43b59fc45ccfada008808239ca0589767930040aa "$overlay" \
  68b932bf23595e217d0b489e2f1267326a19ed4879523fc93f37ba5f28c3ae04 "$source/rust/crates/dev-tools/src/ui_application_lifecycle_recording.rs" \
  4ffa6bc44d1cf2a9f41881541802b779eca279c228f12abaa3a705005b87a7c5 "$source/rust/crates/dev-tools/tests/ui_application_lifecycle_recording.rs" \
  0db8a2dabb0942776afaa73c0a18de2cc84069e2507fd7c8078707cdf3ab0285 "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/source-original.json" | sha256sum --check --strict --quiet -
hold "$source/rust/crates/dev-tools/src/ui_application_lifecycle_recording.rs"
hold "$source/rust/crates/dev-tools/tests/ui_application_lifecycle_recording.rs"
# 원문 fixture pin은 baseline/current 양쪽에서 그대로 사용한다. 재생성 단계는 없다.
printf '%s  %s\n' \
  75d52c8df5a083932b807be7bfda08026d6a62eea177ebae4e8b49d452192503 "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/recording-policy-original.json" \
  05a329ee2078b8c71fc02b832c46428c8fa28b454ee8ad17badbcc63078ebc29 "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/global-recording-original.json" \
  3874eabd4fbe12400ab774a225a80a33bce2ef2c8fdcce6db2436a6da2022568 "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/writer-original.json" \
  d34c575d882566097160bda5ea4060cb98c899e304401b357320c57fcd99c40e "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/dynamic-recording-original.json" \
  b2336047fe4fe05a61a832b44c727fda7b5d3dee388aaaefa62e2e07c65546d0 "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/writer-queue-original.json" \
  35fa7e4ac9d900c33fe1479bd0424448f74879cdf5ff0707c0b0188fb481cf5b "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/original-draw.json.gz" \
  84c211425182eeffd333474ae619cef7575b73e2cb1270cfa0cdbe4d2d6b8afd "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/provenance.json" \
  de58975d68e6e186b87f041d20e8fd5c4390f096c4fb3876b556b98e8b036d9c "$source/rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/source-closure.json" | sha256sum --check --strict --quiet -
add_tree source "$source"
add_tree sdk "$sdk"
add_tree system-headers /usr/include
# 링크/컴파일 provider와 OpenGL의 runtime plugin도 현재 파일로 봉인한다.
for query in cc1 cc1plus collect2 as ld; do
  path=$(/usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout 30s /usr/bin/g++ "-print-prog-name=$query")
  [[ $path == /* ]] || path=$(command -v "$path")
  bind_elf "$path"
done
for path in /usr/lib/gcc/x86_64-linux-gnu/*;do [[ ! -d $path ]] || add_tree "gcc-$(basename "$path")" "$path";done
for path in /usr/lib/x86_64-linux-gnu/dri/*.so;do [[ ! -f $path ]] || bind_elf "$path";done
established=true
admit
run strace-help 30 0 /usr/bin/strace --help
grep -F -- --kill-on-exit "$proof/logs/strace-help.stdout" > "$proof/exitkill-supported.txt"
# image-array-native의 필수 decoder SDK를 기존 standalone stager로 준비한다.
# work/archive는 build8GiB, 설치는 raylib과 tools2GiB를 공유한다. cap/deadline은 늘리지 않는다.
image_sdk_source=$source/rust/crates/image-codecs/native/sdk
for name in stage.rs receipt.rs sources.json CMakeLists.txt dav1d.cmake tiff.cmake; do hold "$image_sdk_source/$name"; done
extra_env=(CARGO_TARGET_DIR="$build/candidate-target")
run image-disk-guard-build 120 0 "$sdk/bin/cargo" build --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-disk-space --bin disk-space-guard --message-format=json
disk_guard=$(artifact "$proof/logs/image-disk-guard-build.stdout" "$source" disk-space-guard bin rust/crates/disk-space/src/main.rs false '[]' disk-space)
copy_elf "$disk_guard" image-disk-guard candidate
run image-stager-build 120 0 "$sdk/bin/rustc" --edition=2021 "$image_sdk_source/stage.rs" -o "$build/candidate-target/stage-image-codecs"
copy_elf "$build/candidate-target/stage-image-codecs" image-codecs-stager candidate
mkdir "$build/image-codecs-archives"
jq -er 'if .schema==1 and (.sources|length)==9 and ([.sources[].archive]|unique|length)==9
  then .sources[] | [.archive,.url,.sha256] | @tsv else error("exact decoder source roster required") end' \
  "$image_sdk_source/sources.json" > "$proof/image-sources.tsv"
while IFS=$'\t' read -r archive url digest; do
  [[ $archive =~ ^[A-Za-z0-9._-]+\.tar\.gz$ && $url == https://* && $digest =~ ^[a-f0-9]{64}$ ]]
  run "image-download-${archive%.tar.gz}" 120 0 /usr/bin/curl --fail --location --proto '=https' --proto-redir '=https' \
    --connect-timeout 20 --max-time 90 --max-filesize 67108864 --output "$build/image-codecs-archives/$archive" "$url"
  printf '%s  %s\n' "$digest" "$build/image-codecs-archives/$archive" | sha256sum --check --strict --quiet -
  hold "$build/image-codecs-archives/$archive"
done < "$proof/image-sources.tsv"
extra_env+=(CHECK_DISK_SPACE_BINARY="$proof/elfs/image-disk-guard")
run image-sdk-stage 600 0 "$proof/elfs/image-codecs-stager" "$build/image-codecs-archives" \
  "$build/image-codecs-work" "$tools/image-codecs" "$image_sdk_source"
hold "$tools/image-codecs/openpilot-image-codecs-sdk.txt"
for name in avif dav1d yuv openjp2 tiff jpeg lzma zstd z; do hold "$tools/image-codecs/lib/lib$name.a"; done
add_tree image-codecs-sdk "$tools/image-codecs"
build_env+=(IMAGE_CODECS_SDK_ROOT="$tools/image-codecs")
extra_env=()
# 현재 checkout은 그대로 두고, 별도 owned clone에만 고정 취소 overlay를 적용한다.
run baseline-clone 180 0 /usr/bin/git clone --no-local --no-hardlinks --no-checkout "$source" "$baseline"
run baseline-checkout 90 0 /usr/bin/git -C "$baseline" checkout --detach "$GITHUB_SHA"
run baseline-overlay-check 30 0 /usr/bin/git -C "$baseline" apply --check "$overlay"
run baseline-overlay 30 0 /usr/bin/git -C "$baseline" apply "$overlay"
printf '%s  %s\n' 5956b91c5a3c36f3ebad60ec962e6e7658ad45178cb4d06d405462e1d461f6c0 "$baseline/rust/crates/dev-tools/src/ui_application_lifecycle_recording.rs" | sha256sum --check --strict --quiet -
printf '%s\n' rust/crates/dev-tools/src/ui_application_lifecycle_recording.rs > "$proof/baseline-diff.expected"
git -C "$baseline" --no-optional-locks diff --name-only > "$proof/baseline-diff.actual"
cmp "$proof/baseline-diff.expected" "$proof/baseline-diff.actual"
git -C "$baseline" --no-optional-locks diff --cached --exit-code
[[ $(git -C "$baseline" --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]]
# tree()는 baseline에도 동일한 .git 제외 규칙을 적용한다. index/HEAD는 별도로 검사한다.
baseline_tree "$proof/baseline.before"
git -C "$baseline" --no-optional-locks ls-files -z > "$proof/baseline-index.before"
baseline_sealed=true
extra_env=(CARGO_TARGET_DIR="$build/baseline-target")
run baseline-build 900 0 "$sdk/bin/cargo" test --manifest-path "$baseline/rust/Cargo.toml" --locked -p openpilot-dev-tools --features image-array-native --test ui_application_lifecycle_recording --no-run --message-format=json
base_bin=$(artifact "$proof/logs/baseline-build.stdout" "$baseline" ui_application_lifecycle_recording test rust/crates/dev-tools/tests/ui_application_lifecycle_recording.rs true '["image-array","image-array-native"]')
copy_elf "$base_bin" baseline-recording baseline
selector=actual_pending_close_drains_three_frames_after_first_write_ack_and_reaps
run baseline-red 180 101 "$proof/elfs/baseline-recording" --exact "$selector" --nocapture --test-threads=1 --color=never
[[ $(grep -Fxc 'pending close lost or changed frame bytes: actual=514560 expected=1543680' "$proof/logs/baseline-red.stderr") == 1 ]]
[[ $(grep -Ec '^test result: FAILED\. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out;' "$proof/logs/baseline-red.stdout") == 1 ]]
[[ $(grep -Fxc "    $selector" "$proof/logs/baseline-red.stdout") == 1 ]]
red=true
baseline_tree "$proof/baseline.after"
cmp "$proof/baseline.before.roster" "$proof/baseline.after.roster"
cmp "$proof/baseline.before.sha256" "$proof/baseline.after.sha256"
# baseline source는 실패 이후 최종 봉인에서도 별도로 재검사한다.
extra_env=(CARGO_TARGET_DIR="$build/candidate-target")
run recording-build 900 0 "$sdk/bin/cargo" test --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --features image-array-native --test ui_application_lifecycle_recording --no-run --message-format=json
record_bin=$(artifact "$proof/logs/recording-build.stdout" "$source" ui_application_lifecycle_recording test rust/crates/dev-tools/tests/ui_application_lifecycle_recording.rs true '["image-array","image-array-native"]')
copy_elf "$record_bin" candidate-recording candidate
run reference-build 900 0 "$sdk/bin/cargo" test --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --features ui-application-lifecycle-native --test ui_application_lifecycle_reference --no-run --message-format=json
ref_bin=$(artifact "$proof/logs/reference-build.stdout" "$source" ui_application_lifecycle_reference test rust/crates/dev-tools/tests/ui_application_lifecycle_reference.rs true '["image-array","image-array-native","ui-application-lifecycle-native"]')
copy_elf "$ref_bin" candidate-reference candidate
run stager-build 900 0 "$sdk/bin/cargo" build --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --bin stage-startup-ui-raylib --message-format=json
stager=$(artifact "$proof/logs/stager-build.stdout" "$source" stage-startup-ui-raylib bin rust/crates/dev-tools/src/bin/stage_startup_ui_raylib.rs false '[]')
copy_elf "$stager" raylib-stager candidate
run raylib-stage 300 0 "$proof/elfs/raylib-stager" --arch x86_64 --output "$tools/raylib" --shared
[[ -d $tools/raylib/include && -f $tools/raylib/lib/libopenpilot-raylib.so ]]
bind_elf "$tools/raylib/lib/libopenpilot-raylib.so"
add_tree raylib "$tools/raylib"
run candidate-green 180 0 "$proof/elfs/candidate-recording" --exact "$selector" --nocapture --test-threads=1 --color=never
ok_test candidate-green 1;green=true
run recording9 240 0 "$proof/elfs/candidate-recording" --nocapture --test-threads=1 --color=never
ok_test recording9 9;suite=true
read -r clippy_base clippy_apparent_base <<< "$(usage "$build" $((8*gib)))"
run clippy 600 0 "$sdk/bin/cargo" clippy --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --features ui-application-lifecycle-native --lib --test ui_application_lifecycle_recording --test ui_application_lifecycle_reference --bin stage-startup-ui-raylib -- -D warnings
read -r clippy_after clippy_apparent_after <<< "$(usage "$build" $((8*gib)))"
((clippy_after-clippy_base<=512*1024*1024 && clippy_apparent_after-clippy_apparent_base<=512*1024*1024));clippy=true
extra_env+=(STARTUP_UI_RAYLIB_ROOT="$tools/raylib" STARTUP_UI_RAYLIB_LIBRARY="$tools/raylib/lib/libopenpilot-raylib.so"
  UI_APPLICATION_FFPROBE=/usr/bin/ffprobe UI_FORMS_FAILURE_OUTPUT="$proof/failures")
[[ -x /usr/bin/ffmpeg && -x /usr/bin/ffprobe && -x /usr/bin/Xvfb && -x /usr/bin/xauth ]]
run five-modes 300 0 /usr/bin/xvfb-run -a -e "$proof/xvfb.stderr" -s '-screen 0 2160x1080x24 -noreset' \
  "$proof/elfs/candidate-reference" --exact five_original_modes_preserve_real_graphics_ticks_and_native_encoder --nocapture --test-threads=1 --color=never
ok_test five-modes 1;graphics=true
passed=true
