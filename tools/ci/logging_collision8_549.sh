#!/usr/bin/env bash
# #549 현재 collision8 전용 후보. 로컬 UNRUN; 원본/expected/old27/퇴역 credit을 변경하지 않는다.
# 고정 interface: admit | oversize-controls | run | final-source. 용량/시간/selector override는 받지 않는다.
set -euo pipefail
set +m
umask 077
export LC_ALL=C
export PATH=/usr/bin:/bin
fail() { printf 'logging collision8: %s\n' "$*" >&2; exit 1; }
[[ $# == 1 && ( $1 == admit || $1 == oversize-controls || $1 == run || $1 == final-source ) ]] || fail 'admit, oversize-controls, run or final-source required'
mode=$1
source_policy=${NATIVE_LOGGING_SOURCE_POLICY-present}
case $source_policy in present|absent) ;; *) fail 'NATIVE_LOGGING_SOURCE_POLICY must be present or absent' ;; esac
source_policy_env=()
if [[ $source_policy == absent ]]; then source_policy_env=(NATIVE_LOGGING_SOURCE_POLICY=absent); fi
[[ ${GITHUB_ACTIONS:-} == true && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust &&
   ${GITHUB_EVENT_NAME:-} == push && ${GITHUB_REF:-} == refs/heads/codex/feat-523-python-rust &&
   ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:X64 &&
   $(uname -m) == x86_64 && ${GITHUB_SHA:-} =~ ^[a-f0-9]{40}$ ]] || fail 'trusted hosted source required'
source=${DIAG_SOURCE:?}
job=${CI_JOB_TEMP:?}
owner=${DIAG_SOURCE_OWNER:?}
workspace=${DIAG_ACTIONS_WORKSPACE:?}
case "$owner" in "$workspace/.collision549-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."??????) ;; *) fail 'source owner name differs' ;; esac
[[ $job == /tmp/openpilot-ci.?????? && $PWD == "$source" && ${GITHUB_WORKSPACE:-} == "$source" &&
   $source == "$owner/source" && ${CARGO_HOME:-} == "$job/build/cargo-home" &&
   ${CARGO_TARGET_DIR:-} == "$job/build/target" && ${RUSTUP_HOME:-} == "$job/rustup-home" ]] || fail 'private layout differs'
[[ $source =~ ^/[A-Za-z0-9_./-]+$ && $job =~ ^/[A-Za-z0-9_./-]+$ ]] || fail 'non-ASCII private path'
for path in "$source" "$owner" "$job" "$job/proof"; do
  [[ -d $path && ! -L $path && $(readlink -e -- "$path") == "$path" && $(stat -c %u -- "$path") == "$(id -u)" ]] || fail 'owned canonical root required'
done
for path in "$job/owner" "$owner/.diag296-owner"; do
  [[ -f $path && ! -L $path && $(stat -c %u "$path") == "$(id -u)" ]] || fail 'owner marker unsafe'
done
[[ $(stat -c %a "$job") == 700 && $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(cat "$owner/.diag296-owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(stat -c '%d:%i' "$owner") == "$DIAG_SOURCE_IDENTITY" ]] || fail 'owner differs'
[[ $(git --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || fail 'source HEAD differs'
[[ ${COLLISION8_HELPER_SHA256:-} =~ ^[a-f0-9]{64}$ ]] || fail 'helper pin missing'
[[ ${COLLISION8_BATCHES_SHA256:-} =~ ^[a-f0-9]{64}$ ]] || fail 'batch worker pin missing'
[[ $(readlink -e "${BASH_SOURCE[0]}") == "$source/tools/ci/logging_collision8_549.sh" ]] || fail 'helper source path differs'
printf '%s  %s\n' "$COLLISION8_HELPER_SHA256" "$source/tools/ci/logging_collision8_549.sh" | sha256sum --check --strict --quiet -
batch_worker=$source/tools/ci/logging_collision8_batches_549.sh
[[ -f $batch_worker && ! -L $batch_worker ]]
printf '%s  %s\n' "$COLLISION8_BATCHES_SHA256" "$batch_worker" | sha256sum --check --strict --quiet -
unset GH_TOKEN GITHUB_TOKEN NATIVE_LOGGING_LIFECYCLE_RESUME NATIVE_LOGGING_TEST_BUILDER NATIVE_LOGGING_TEST_REFERENCE
unset LD_PRELOAD LD_LIBRARY_PATH BASH_ENV ENV RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_TARGET RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
export GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1
proof=$job/proof/c8
source_policy_baseline=$job/proof/c8-source-policy.parents.tsv
source_policy_metadata=$job/proof/c8-source-policy.json
source_policy_failure=$job/proof/c8-source-policy.failed
# 실제 absent phase의 checkpoint guard다. CLI 내부 SourceGuard의 NOFOLLOW/ENOENT 검사를 대신하지 않는다.
source_policy_ready=false
source_policy_paths=() source_policy_fds=() source_policy_ids=()
source_policy_observe() {
  local i path current nodes index_entries head_entries
  [[ $source_policy_ready == true && ! -e $source_policy_failure && ! -L $source_policy_failure ]] || return 1
  for i in "${!source_policy_paths[@]}"; do
    path=${source_policy_paths[i]}
    [[ -d $path && ! -L $path && $(realpath -e -- "$path") == "$path" ]] || return 1
    current=$(stat -c '%d:%i:%f:%u:%g' -- "$path") || return 1
    [[ $current == "${source_policy_ids[i]}" &&
       $(stat -Lc '%d:%i:%f:%u:%g' -- "/proc/$$/fd/${source_policy_fds[i]}") == "$current" ]] || return 1
  done
  # complete name scan은 모든 오류를 거부한다. dangling symlink도 이름이 있으면 재등장이다.
  nodes=$(/usr/bin/find -P "$source/rust/tools" -mindepth 1 -maxdepth 1 \
    \( -name check_native_logging.py -o -name native_logging_transport.py \) -printf '%f\n') || return 1
  [[ -z $nodes ]] || return 1
  index_entries=$(/usr/bin/git --no-optional-locks -C "$source" ls-files -- rust/tools/check_native_logging.py rust/tools/native_logging_transport.py) || return 1
  head_entries=$(/usr/bin/git --no-optional-locks -C "$source" ls-tree -r --name-only HEAD -- rust/tools/check_native_logging.py rust/tools/native_logging_transport.py) || return 1
  [[ -z $index_entries && -z $head_entries ]] || return 1
  for i in "${!source_policy_paths[@]}"; do
    [[ ! -L ${source_policy_paths[i]} &&
       $(stat -c '%d:%i:%f:%u:%g' -- "${source_policy_paths[i]}") == "${source_policy_ids[i]}" &&
       $(stat -Lc '%d:%i:%f:%u:%g' -- "/proc/$$/fd/${source_policy_fds[i]}") == "${source_policy_ids[i]}" ]] || return 1
  done
}
source_policy_checkpoint() {
  [[ $source_policy == absent ]] || return 0
  if ! source_policy_observe; then
    # 첫 관측 실패는 다른 checkpoint/프로세스가 성공해도 지우거나 덮어쓰지 않는다.
    (set -C; printf 'source_policy=absent\nsource_commit=%s\nphase=%s\nresult=FAIL\n' "$GITHUB_SHA" "${phase:-admission}" > "$source_policy_failure") 2>/dev/null || true
    printf 'logging source absence checkpoint failed\n' >&2
    return 1
  fi
}
source_policy_init() {
  local path fd identity rows= mode_value actual expected
  [[ $source_policy == absent ]] || return 0
  for path in "$source" "$source/rust" "$source/rust/tools"; do
    [[ -d $path && ! -L $path && $(realpath -e -- "$path") == "$path" && $(stat -c %u -- "$path") == "$UID" ]] || return 1
    mode_value=$(stat -c %a -- "$path") || return 1
    (( (8#$mode_value & 0022) == 0 )) || return 1
    exec {fd}< "$path" || return 1
    identity=$(stat -Lc '%d:%i:%f:%u:%g' -- "/proc/$$/fd/$fd") || return 1
    [[ $(stat -c '%d:%i:%f:%u:%g' -- "$path") == "$identity" ]] || return 1
    source_policy_paths+=("$path"); source_policy_fds+=("$fd"); source_policy_ids+=("$identity")
    printf -v rows '%s%s\t%s\n' "$rows" "$path" "$identity"
  done
  expected=$(printf '{"schema":1,"source_commit":"%s","source_policy":"absent","scope":"sampled source checkpoints; CLI owns NOFOLLOW/ENOENT checks","postdelete_full_acceptance":false,"R4_acceptance":false,"retirement_credit":0}' "$GITHUB_SHA")
  for path in "$source_policy_baseline" "$source_policy_metadata"; do
    if [[ -e $path || -L $path ]]; then
      [[ -f $path && ! -L $path && $(stat -c '%u:%h' -- "$path") == "$UID:1" && $(stat -c %s -- "$path") -le 4096 ]] || return 1
    fi
  done
  if [[ ! -e $source_policy_baseline ]]; then (set -C; printf '%s' "$rows" > "$source_policy_baseline") || return 1; fi
  if [[ ! -e $source_policy_metadata ]]; then (set -C; printf '%s\n' "$expected" > "$source_policy_metadata") || return 1; fi
  actual=$(cat -- "$source_policy_baseline") || return 1
  [[ "$actual" == "${rows%$'\n'}" && $(cat -- "$source_policy_metadata") == "$expected" ]] || return 1
  source_policy_ready=true
  source_policy_checkpoint
}

build=$job/build
sdk=$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu
gib=$((1024 * 1024 * 1024))
reserve=$((512 * 1024 * 1024))
scan_role=main

free_bytes() {
  df --output=avail -B1 -- "$1" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {n++;v=$1} END {if(NR!=2 || n!=1) exit 1; print v}'
}
# 실패 뒤 final admit이 role별 scratch를 덮기 전에 원문을 별도 create-new 기록으로 남긴다.
# 진단 저장 실패도 원래 admission 거부를 성공으로 바꾸지 않는다.
admission_failure() {
  local c8_failure_code=$1 c8_failure_stage=$2 c8_failure_root=$3 c8_failure_detail=$4
  local c8_failure_base c8_failure_input c8_failure_index=0 c8_failure_size
  local c8_failure_pid=$BASHPID c8_failure_time=$EPOCHREALTIME
  shift 4
  c8_failure_base=$job/proof/c8-admission-failure.$scan_role.$c8_failure_pid.$c8_failure_time
  (
    set -C
    printf 'phase=%s\nrole=%s\npid=%s\ntime=%s\nstage=%s\nroot=%s\ncode=%s\nadmission_exit=1\ndetail=%s\n' \
      "${phase:-$mode}" "$scan_role" "$c8_failure_pid" "$c8_failure_time" "$c8_failure_stage" "$c8_failure_root" "$c8_failure_code" "$c8_failure_detail" > "$c8_failure_base.meta" || exit 1
    for c8_failure_input in "$@"; do
      c8_failure_index=$((c8_failure_index+1))
      printf 'snapshot_%s_path=%s\n' "$c8_failure_index" "$c8_failure_input" >> "$c8_failure_base.meta" || exit 1
      [[ -f $c8_failure_input && ! -L $c8_failure_input ]] || exit 1
      c8_failure_size=$(stat -c %s "$c8_failure_input") || exit 1
      [[ $c8_failure_size =~ ^[0-9]+$ ]] && ((c8_failure_size<=536870912)) || exit 1
      cat -- "$c8_failure_input" > "$c8_failure_base.$c8_failure_index.raw" || exit 1
    done
    printf 'snapshots_complete=true\n' >> "$c8_failure_base.meta"
  ) || printf 'logging collision8: admission diagnostic incomplete: %s\n' "$c8_failure_base" >&2
  return 0
}
# 실제 find/compiler를 모사하지 않고 동일 판정 함수의 authored 입력 경계만 검사한다.
oversize_control() {
  local c8_control_name=$1 c8_control_root=$2 c8_control_before=$3 c8_control_code=$4
  local c8_control_errors=$5 c8_control_list=$6 c8_control_expected_code=$7 c8_control_expected_reason=$8
  local c8_control_actual_code=0 c8_control_actual_reason
  c8_control_actual_reason=$(oversize_result "$c8_control_root" "$c8_control_before" "$c8_control_code" "$c8_control_errors" "$c8_control_list") || c8_control_actual_code=$?
  printf '%s\t%s\t%s\t%s\t%s\n' "$c8_control_name" "$c8_control_actual_code" "$c8_control_actual_reason" "$c8_control_expected_code" "$c8_control_expected_reason" >> "$job/proof/c8-oversize-controls/results.tsv" || return 1
  [[ $c8_control_actual_code == "$c8_control_expected_code" && $c8_control_actual_reason == "$c8_control_expected_reason" ]]
}
oversize_controls() {
  local c8_controls=$job/proof/c8-oversize-controls c8_controls_identity c8_controls_relative
  # 이 mode의 synthetic root뿐이다. 실제 run의 전역 build/target은 생성·변경하지 않는다.
  local build=$job/proof/c8-oversize-controls/build
  mkdir "$c8_controls" "$build" "$build/target" "$build/target/debug" "$build/target/debug/deps" "$c8_controls/registry" || return 1
  (
    set -C
    : > "$c8_controls/results.tsv" || exit 1
    : > "$c8_controls/empty.stderr" || exit 1
    : > "$c8_controls/empty.txt" || exit 1
    : > "$build/target/debug/deps/present.o" || exit 1
    ln -s missing-target "$build/target/debug/deps/dangling.o" || exit 1
    for c8_controls_relative in \
      debug/deps/rustls-329f250fb5c1037d.rustls.93c795f2bef50a4-cgu.02.rcgu.o \
      debug/deps/rustls-329f250fb5c1037d.rustls.93c795f2bef50a4-cgu.03.rcgu.o \
      debug/deps/rustls-329f250fb5c1037d.rustls.93c795f2bef50a4-cgu.00.rcgu.o \
      debug/deps/rustcVcJzBi \
      debug/deps/rustls-329f250fb5c1037d.rustls.93c795f2bef50a4-cgu.01.rcgu.o; do
      printf "find: '%s/target/%s': No such file or directory\n" "$build" "$c8_controls_relative"
    done > "$c8_controls/historical-shape.stderr" || exit 1
    printf "find: '%s/outside': No such file or directory\n" "$c8_controls" > "$c8_controls/outside.stderr" || exit 1
    printf "find: '%s/target/debug/deps/missing.o': Permission denied\n" "$build" > "$c8_controls/permission.stderr" || exit 1
    printf "find: '%s/target/debug/deps/present.o': No such file or directory\n" "$build" > "$c8_controls/present.stderr" || exit 1
    printf "find: '%s/target/debug/deps/dangling.o': No such file or directory\n" "$build" > "$c8_controls/dangling.stderr" || exit 1
    printf "find: '%s/target/../missing.o': No such file or directory\n" "$build" > "$c8_controls/parent.stderr" || exit 1
    printf "find: '%s/target/./missing.o': No such file or directory\n" "$build" > "$c8_controls/dot.stderr" || exit 1
    printf "find: '%s/target/debug//missing.o': No such file or directory\n" "$build" > "$c8_controls/slashes.stderr" || exit 1
    printf "find: '%s/registry/missing': No such file or directory\n" "$c8_controls" > "$c8_controls/registry.stderr" || exit 1
    cat "$c8_controls/historical-shape.stderr" "$c8_controls/permission.stderr" > "$c8_controls/mixed.stderr" || exit 1
    # 양성 목록은 authored scanner output이며 512MiB 실제 파일을 만들거나 검사했다고 세지 않는다.
    printf '%s/target/debug/deps/oversized.o\n' "$build" > "$c8_controls/oversize.txt" || exit 1
    c8_controls_identity=$(stat -c '%d:%i' "$build") || exit 1
    oversize_control clean "$build" "$c8_controls_identity" 0 "$c8_controls/empty.stderr" "$c8_controls/empty.txt" 0 clean || exit 1
    oversize_control historical-five-enoent "$build" "$c8_controls_identity" 1 "$c8_controls/historical-shape.stderr" "$c8_controls/empty.txt" 75 retryable-target-enoent || exit 1
    oversize_control outside-path "$build" "$c8_controls_identity" 1 "$c8_controls/outside.stderr" "$c8_controls/empty.txt" 1 retry-line || exit 1
    oversize_control permission "$build" "$c8_controls_identity" 1 "$c8_controls/permission.stderr" "$c8_controls/empty.txt" 1 retry-line || exit 1
    oversize_control existing-path "$build" "$c8_controls_identity" 1 "$c8_controls/present.stderr" "$c8_controls/empty.txt" 1 retry-present || exit 1
    oversize_control dangling-path "$build" "$c8_controls_identity" 1 "$c8_controls/dangling.stderr" "$c8_controls/empty.txt" 1 retry-present || exit 1
    oversize_control parent-traversal "$build" "$c8_controls_identity" 1 "$c8_controls/parent.stderr" "$c8_controls/empty.txt" 1 retry-components || exit 1
    oversize_control dot-component "$build" "$c8_controls_identity" 1 "$c8_controls/dot.stderr" "$c8_controls/empty.txt" 1 retry-components || exit 1
    oversize_control duplicate-slash "$build" "$c8_controls_identity" 1 "$c8_controls/slashes.stderr" "$c8_controls/empty.txt" 1 retry-components || exit 1
    oversize_control positive-partial-enoent "$build" "$c8_controls_identity" 1 "$c8_controls/historical-shape.stderr" "$c8_controls/oversize.txt" 1 oversize-file || exit 1
    oversize_control positive-clean-status "$build" "$c8_controls_identity" 0 "$c8_controls/empty.stderr" "$c8_controls/oversize.txt" 1 oversize-file || exit 1
    oversize_control proof-root "$job/proof" "$(stat -c '%d:%i' "$job/proof")" 1 "$c8_controls/historical-shape.stderr" "$c8_controls/empty.txt" 1 scan-status || exit 1
    oversize_control registry-error "$build" "$c8_controls_identity" 1 "$c8_controls/registry.stderr" "$c8_controls/empty.txt" 1 retry-line || exit 1
    oversize_control other-exit "$build" "$c8_controls_identity" 2 "$c8_controls/historical-shape.stderr" "$c8_controls/empty.txt" 1 scan-status || exit 1
    oversize_control root-identity "$build" 0:0 0 "$c8_controls/empty.stderr" "$c8_controls/empty.txt" 1 root-identity || exit 1
    oversize_control success-with-stderr "$build" "$c8_controls_identity" 0 "$c8_controls/historical-shape.stderr" "$c8_controls/empty.txt" 1 scan-status || exit 1
    oversize_control failure-without-stderr "$build" "$c8_controls_identity" 1 "$c8_controls/empty.stderr" "$c8_controls/empty.txt" 1 scan-status || exit 1
    oversize_control mixed-errors "$build" "$c8_controls_identity" 1 "$c8_controls/mixed.stderr" "$c8_controls/empty.txt" 1 retry-line || exit 1
    jq -Rn --arg source "$GITHUB_SHA" --arg helper "$COLLISION8_HELPER_SHA256" \
      '[inputs | split("\t") | {name:.[0],code:(.[1]|tonumber),reason:.[2],expected_code:(.[3]|tonumber),expected_reason:.[4]}] |
       if length!=18 or ([.[].name]|unique|length)!=18 or any(.[]; .code!=.expected_code or .reason!=.expected_reason) then error("control roster/result differs") else
       {schema:1,source_commit:$source,helper_sha256:$helper,synthetic_only:true,actual_collision8_admitted:false,all_pass:true,count:length,controls:.} end' \
      < "$c8_controls/results.tsv" > "$c8_controls/complete.json"
  )
}
# 0=깨끗한 전체 scan, 75=한정 target ENOENT 재관측, 1=즉시 거부.
# 양성 목록을 제일 먼저 거부하므로 부분 결과가 다음 scan에서 사라져도 PASS가 되지 않는다.
oversize_result() {
  local c8_result_root=$1 c8_result_before=$2 c8_result_code=$3 c8_result_errors=$4 c8_result_list=$5
  local c8_result_line c8_result_relative c8_result_size
  local c8_result_prefix="find: '$build/target/" c8_result_suffix="': No such file or directory"
  [[ -f $c8_result_list && ! -L $c8_result_list && -f $c8_result_errors && ! -L $c8_result_errors ]] || { printf 'scan-evidence\n'; return 1; }
  [[ ! -s $c8_result_list ]] || { printf 'oversize-file\n'; return 1; }
  [[ -d $c8_result_root && ! -L $c8_result_root && $(readlink -e "$c8_result_root") == "$c8_result_root" &&
     $(stat -c '%d:%i' "$c8_result_root") == "$c8_result_before" ]] || { printf 'root-identity\n'; return 1; }
  if [[ $c8_result_code == 0 && ! -s $c8_result_errors ]]; then printf 'clean\n'; return 0; fi
  [[ $c8_result_root == "$build" && $c8_result_code == 1 && -s $c8_result_errors ]] || { printf 'scan-status\n'; return 1; }
  c8_result_size=$(stat -c %s "$c8_result_errors") || { printf 'scan-evidence\n'; return 1; }
  [[ $c8_result_size =~ ^[0-9]+$ ]] && ((c8_result_size<=536870912)) || { printf 'scan-evidence\n'; return 1; }
  # Bash read가 NUL을 버려 잘못된 원문을 정규화하지 않도록 먼저 원문 byte를 확인한다.
  if ! tr -d '\000' < "$c8_result_errors" | cmp -s - "$c8_result_errors"; then printf 'retry-line\n'; return 1; fi
  while IFS= read -r c8_result_line || [[ -n $c8_result_line ]]; do
    [[ $c8_result_line == "$c8_result_prefix"*"$c8_result_suffix" ]] || { printf 'retry-line\n'; return 1; }
    c8_result_relative=${c8_result_line#"$c8_result_prefix"}; c8_result_relative=${c8_result_relative%"$c8_result_suffix"}
    [[ $c8_result_relative =~ ^[A-Za-z0-9_./+-]+$ ]] || { printf 'retry-path\n'; return 1; }
    case "/$c8_result_relative/" in *'/../'*|*'/./'*|*'//'*) printf 'retry-components\n'; return 1;; esac
    [[ ! -e $build/target/$c8_result_relative && ! -L $build/target/$c8_result_relative ]] || { printf 'retry-present\n'; return 1; }
  done < "$c8_result_errors"
  printf 'retryable-target-enoent\n'
  return 75
}
oversize_scan() {
  local c8_scan_root=$1 c8_scan_before c8_scan_after c8_scan_attempt c8_scan_code c8_scan_decision c8_scan_reason
  local c8_scan_base c8_scan_errors c8_scan_list c8_scan_list_fd c8_scan_error_fd
  [[ $c8_scan_root == "$build" || $c8_scan_root == "$job/proof" ]] || { admission_failure 1 admit.oversize-root "$c8_scan_root" 'unapproved root'; return 1; }
  [[ -d $c8_scan_root && ! -L $c8_scan_root && $(readlink -e "$c8_scan_root") == "$c8_scan_root" ]] || { admission_failure 1 admit.oversize-root "$c8_scan_root" 'canonical directory required'; return 1; }
  c8_scan_before=$(stat -c '%d:%i' "$c8_scan_root") || { admission_failure "$?" admit.oversize-identity-before "$c8_scan_root" 'command=stat'; return 1; }
  for c8_scan_attempt in 1 2 3; do
    [[ -d $c8_scan_root && ! -L $c8_scan_root && $(readlink -e "$c8_scan_root") == "$c8_scan_root" &&
       $(stat -c '%d:%i' "$c8_scan_root") == "$c8_scan_before" ]] || { admission_failure 1 admit.oversize-identity-before "$c8_scan_root" "attempt=$c8_scan_attempt before=$c8_scan_before"; return 1; }
    c8_scan_base=$job/proof/c8-oversize.$scan_role.$BASHPID.$EPOCHREALTIME.$c8_scan_attempt
    c8_scan_errors=$c8_scan_base.stderr
    c8_scan_list=$c8_scan_base.txt
    c8_scan_code=0
    (
      set -C
      exec {c8_scan_list_fd}> "$c8_scan_list" || exit 125
      exec {c8_scan_error_fd}> "$c8_scan_errors" || exit 125
      find -P "$c8_scan_root" -type f -size +536870912c -print >&"$c8_scan_list_fd" 2>&"$c8_scan_error_fd"
    ) || c8_scan_code=$?
    c8_scan_after=$(stat -c '%d:%i' "$c8_scan_root") || c8_scan_after=unavailable
    c8_scan_decision=0
    c8_scan_reason=$(oversize_result "$c8_scan_root" "$c8_scan_before" "$c8_scan_code" "$c8_scan_errors" "$c8_scan_list") || c8_scan_decision=$?
    # 첫 scan부터 모든 원문을 고유 경로로 유지한다. final admit도 이전 시도를 덮지 않는다.
    (
      set -C
      printf 'phase=%s\nrole=%s\nattempt=%s\nroot=%s\nroot_before=%s\nroot_after=%s\nfind_code=%s\ndecision=%s\nreason=%s\nstderr=%s\nlist=%s\n' \
        "${phase:-$mode}" "$scan_role" "$c8_scan_attempt" "$c8_scan_root" "$c8_scan_before" "$c8_scan_after" "$c8_scan_code" "$c8_scan_decision" "$c8_scan_reason" "$c8_scan_errors" "$c8_scan_list" > "$c8_scan_base.meta" || exit 1
      if [[ $c8_scan_attempt == 1 && $c8_scan_decision != 0 ]]; then
        printf 'first_failed_scan=%s\nfind_code=%s\ndecision=%s\nreason=%s\n' "$c8_scan_base.meta" "$c8_scan_code" "$c8_scan_decision" "$c8_scan_reason" > "$c8_scan_base.first-failure" || exit 1
      fi
    ) || { admission_failure 1 admit.oversize-journal "$c8_scan_root" "attempt=$c8_scan_attempt find_code=$c8_scan_code" "$c8_scan_errors" "$c8_scan_list"; return 1; }
    [[ $c8_scan_after == "$c8_scan_before" ]] || { admission_failure 1 admit.oversize-identity-after "$c8_scan_root" "attempt=$c8_scan_attempt find_code=$c8_scan_code before=$c8_scan_before after=$c8_scan_after" "$c8_scan_errors" "$c8_scan_list"; return 1; }
    if [[ $c8_scan_decision == 0 ]]; then return 0; fi
    if [[ $c8_scan_decision != 75 ]]; then
      admission_failure "$c8_scan_code" "admit.oversize-$c8_scan_reason" "$c8_scan_root" "attempt=$c8_scan_attempt find_code=$c8_scan_code decision=$c8_scan_decision file_cap=536870912" "$c8_scan_errors" "$c8_scan_list"
      return 1
    fi
  done
  admission_failure "$c8_scan_code" admit.oversize-retry-exhausted "$c8_scan_root" 'attempts=3 file_cap=536870912' "$c8_scan_errors" "$c8_scan_list"
  return 1
}
# mutable target의 정확한 ENOENT만 3회 재관측한다. 부분 합의 초과는 즉시 실패한다.
usage() {
  local root=$1 cap=$2 attempt code values bytes apparent line relative before
  local errors=$job/proof/c8-scan-$scan_role.stderr
  local prefix="find: '$build/target/" suffix="': No such file or directory"
  if [[ ! -e $root && ! -L $root ]]; then printf '0 0\n'; return; fi
  [[ -d $root && ! -L $root && $(readlink -e "$root") == "$root" ]] || { admission_failure "$?" usage.root "$root" "cap=$cap"; return 1; }
  before=$(stat -c '%d:%i' "$root") || { admission_failure "$?" usage.identity-before "$root" "cap=$cap"; return 1; }
  for attempt in 1 2 3; do
    code=0
    values=$(
      set +e
      find -P "$root" -ignore_readdir_race -printf '%b %s\n' 2> "$errors" |
        awk '{b+=$1*512;s+=$2} END {printf "%.0f %.0f\n",b,s}'
      codes=("${PIPESTATUS[@]}")
      ((codes[1] == 0)) || { admission_failure "${codes[1]}" usage.awk "$root" "attempt=$attempt find_code=${codes[0]} awk_code=${codes[1]}" "$errors"; exit 125; }
      exit "${codes[0]}"
    ) || code=$?
    [[ $(stat -c '%d:%i' "$root") == "$before" ]] || { admission_failure "$?" usage.identity-after "$root" "attempt=$attempt find_pipeline_code=$code before=$before" "$errors"; return 1; }
    if [[ $code == 0 || $code == 1 ]]; then
      read -r bytes apparent <<< "$values"
      [[ $bytes =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]] || { admission_failure "$?" usage.values "$root" "attempt=$attempt find_pipeline_code=$code values=$values" "$errors"; return 1; }
      ((bytes <= cap && apparent <= cap)) || { admission_failure "$?" usage.cap "$root" "attempt=$attempt find_pipeline_code=$code cap=$cap allocated=$bytes apparent=$apparent" "$errors"; return 1; }
    fi
    if [[ $code == 0 && ! -s $errors ]]; then printf '%s\n' "$values"; return; fi
    [[ $root == "$build" && $code == 1 && -s $errors ]] || { admission_failure "$code" usage.scan-status "$root" "attempt=$attempt stderr_not_empty=$([[ -s $errors ]] && printf true || printf false)" "$errors"; return 1; }
    while IFS= read -r line || [[ -n $line ]]; do
      [[ $line == "$prefix"*"$suffix" ]] || { admission_failure "$?" usage.retry-line "$root" "attempt=$attempt find_pipeline_code=$code" "$errors"; return 1; }
      relative=${line#"$prefix"}; relative=${relative%"$suffix"}
      [[ $relative =~ ^[A-Za-z0-9_./+-]+$ ]] || { admission_failure "$?" usage.retry-path "$root" "attempt=$attempt find_pipeline_code=$code" "$errors"; return 1; }
      case "/$relative/" in *'/../'*|*'/./'*|*'//'*) admission_failure 1 usage.retry-components "$root" "attempt=$attempt find_pipeline_code=$code" "$errors"; return 1;; esac
      [[ ! -e $build/target/$relative && ! -L $build/target/$relative ]] || { admission_failure "$?" usage.retry-present "$root" "attempt=$attempt find_pipeline_code=$code" "$errors"; return 1; }
    done < "$errors"
    cat "$errors" >> "$job/proof/c8-budget-races.log" || { admission_failure "$?" usage.retry-journal "$root" "attempt=$attempt find_pipeline_code=$code" "$errors"; return 1; }
  done
  admission_failure 1 usage.retry-exhausted "$root" "attempts=3 find_pipeline_code=$code" "$errors"
  return 1
}
admit() {
  if ! source_policy_checkpoint; then
    admission_failure 1 admit.source-policy "$source" 'absent source checkpoint failed'
    return 1
  fi
  local path cap used apparent remaining device available n sdk_used=0 sdk_apparent=0
  local -A needs=() representatives=()
  local -a paths=("$owner" "$build" "$job/proof") caps=($((2*gib)) $((8*gib)) "$gib")
  [[ $(stat -c %d "$source") == "$(stat -c %d "$owner")" ]] || { admission_failure "$?" admit.source-device "$source" "owner=$owner"; return 1; }
  for path in "$build" "$job/proof" "$job/rustup-home" "$job/cargo-home" "$job/rust-bootstrap"; do
    if [[ -e $path ]]; then [[ $(stat -c %d "$path") == "$(stat -c %d "$job")" ]] || { admission_failure "$?" admit.job-device "$path" "job=$job"; return 1; }; fi
  done
  for n in "${!paths[@]}"; do
    path=${paths[$n]}; cap=${caps[$n]}
    read -r used apparent <<< "$(usage "$path" "$cap")" || { admission_failure "$?" admit.usage-read "$path" "cap=$cap"; return 1; }
    [[ $used =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]] || { admission_failure "$?" admit.usage-values "$path" "cap=$cap allocated=$used apparent=$apparent"; return 1; }
    remaining=$((cap-used))
    case "$path" in "$owner") device=$(stat -c %d "$owner"); representatives[$device]=$owner ;;
      *) device=$(stat -c %d "$job"); representatives[$device]=$job ;; esac
    needs[$device]=$(( ${needs[$device]:-0}+remaining ))
  done
  for path in "$job/rustup-home" "$job/cargo-home" "$job/rust-bootstrap"; do
    read -r used apparent <<< "$(usage "$path" $((3*gib)))" || { admission_failure "$?" admit.sdk-read "$path" "cap=$((3*gib))"; return 1; }
    [[ $used =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]] || { admission_failure "$?" admit.sdk-values "$path" "cap=$((3*gib)) allocated=$used apparent=$apparent"; return 1; }
    sdk_used=$((sdk_used+used)); sdk_apparent=$((sdk_apparent+apparent))
    ((sdk_used <= 3*gib && sdk_apparent <= 3*gib)) || { admission_failure "$?" admit.sdk-sum "$path" "cap=$((3*gib)) allocated=$sdk_used apparent=$sdk_apparent"; return 1; }
  done
  device=$(stat -c %d "$job")
  needs[$device]=$(( ${needs[$device]:-0}+3*gib-sdk_used ))
  if [[ ! -e $job/proof/c8-packages.done ]]; then
    device=$(stat -c %d /); representatives[$device]=/
    needs[$device]=$(( ${needs[$device]:-0}+2*gib ))
  else
    [[ -f $job/proof/c8-packages.done && ! -L $job/proof/c8-packages.done && $(stat -c %s "$job/proof/c8-packages.done") == 41 && $(cat "$job/proof/c8-packages.done") == "$GITHUB_SHA" ]] || { admission_failure "$?" admit.package-marker "$job/proof/c8-packages.done" "source=$GITHUB_SHA"; return 1; }
  fi
  for device in "${!needs[@]}"; do
    available=$(free_bytes "${representatives[$device]}") || { admission_failure "$?" admit.free-read "${representatives[$device]}" "device=$device"; return 1; }
    [[ $available =~ ^[0-9]+$ ]] || { admission_failure "$?" admit.free-value "${representatives[$device]}" "device=$device available=$available"; return 1; }
    printf '%s\t%s\t%s\t%s\n' "$EPOCHREALTIME" "$device" "$available" "$((needs[$device]+reserve))" >> "$job/proof/c8-admission.tsv" || { admission_failure "$?" admit.free-journal "${representatives[$device]}" "device=$device"; return 1; }
    ((available >= needs[$device]+reserve)) || { admission_failure "$?" admit.free-cap "${representatives[$device]}" "device=$device available=$available required=$((needs[$device]+reserve))"; return 1; }
  done
  # foreign symlink/socket은 collision 본문 자료다. follow하지 않으며 실제 regular-file 상한만 검사한다.
  for path in "$job/proof" "$build"; do
    if [[ -d $path ]]; then
      oversize_scan "$path" || return 1
    fi
  done
}
originals() {
  if [[ $source_policy == absent ]]; then source_policy_checkpoint; return; fi
  local relative
  for relative in rust/tools/check_native_logging.py rust/tools/native_logging_transport.py; do
    [[ -f $source/$relative && ! -L $source/$relative ]] || return 1
  done
  printf '%s  %s\n' adee1d45e299bfd6448e8f5584b194b2dbbb53622ee3c1d57c02538d0b0d1fc4 "$source/rust/tools/check_native_logging.py" \
    288a4fd0f9aca05d82a3f415e6632ef3e62b92f2d73f01d7a2f1b5263cdb7ad1 "$source/rust/tools/native_logging_transport.py" | sha256sum --check --strict --quiet -
}
source_final() {
  local status=0
  git ls-files -z > "$job/proof/index-after.bin" || status=1
  cmp "$job/proof/index-before.bin" "$job/proof/index-after.bin" || status=1
  tar --null --no-recursion --verbatim-files-from -T "$job/proof/index-before.bin" -cf - \
    --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner | sha256sum > "$job/proof/source-after.sha256" || status=1
  cmp "$job/proof/source-before.sha256" "$job/proof/source-after.sha256" || status=1
  git diff --exit-code || status=1
  git diff --cached --exit-code || status=1
  originals || status=1
  admit || status=1
  return "$status"
}
source_policy_init
if [[ $mode == admit ]]; then admit; exit; fi
if [[ $mode == oversize-controls ]]; then admit; oversize_controls; admit; exit; fi
if [[ $mode == final-source ]]; then source_final; exit; fi

[[ ${CI_PREPARED_RUST_ROOT:-} == "$sdk" && ${CI_PREPARED_RUST_MANIFEST:-} == "$job/proof/prepared-rust-files.before.sha256" ]] || fail 'prepared SDK binding differs'
export PATH="$sdk/bin:/usr/bin:/bin"
mkdir "$proof" "$proof/build" "$proof/cargo-trace" "$proof/reference-trace" "$proof/case-trace" "$proof/cases" "$proof/inspect"
mkdir "$build" "$build/cargo-home" "$build/target" "$build/tmp"
export TMPDIR=$build/tmp
phase=preflight
passed=false
baseline=false
active= active_identity= watcher= stop=
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
    [[ $(readlink -e "$path") == "${aliases[$path]}" && $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") == "${alias_id[$path]}" ]] || {
      printf 'held alias identity differs: %s\n' "$path" >&2; status=1;
    }
  done
  for path in "${!held_fd[@]}"; do
    fd=${held_fd[$path]}
    [[ $(idfile "$path") == "${held_id[$path]}" && $(idfile "/proc/$$/fd/$fd") == "${held_id[$path]}" ]] || {
      printf 'held path/FD identity differs: %s\n' "$path" >&2; status=1; continue;
    }
    hash=$(sha256sum "/proc/$$/fd/$fd") || { printf 'held FD hash read failed: %s\n' "$path" >&2; status=1; continue; }; hash=${hash%% *}
    [[ $hash == "${held_sha[$path]}" && $(idfile "$path") == "${held_id[$path]}" ]] || {
      printf 'held hash/final identity differs: %s\n' "$path" >&2; status=1;
    }
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
# ldd/loader를 통한 target 실행은 없다. ELF를 읽어 고정 search roots의 unique data provider만 묶는다.
bind_elf() {
  local path canonical needed root resolved candidate interp
  path=$1; hold "$path" || return 1
  canonical=$(readlink -e "$path") || return 1
  [[ -v elf_seen[$canonical] ]] && return 0
  elf_seen[$canonical]=true
  [[ $(head -c 4 "$canonical" | od -An -tx1 | tr -d ' \n') == 7f454c46 ]] || return 1
  readelf -h "$canonical" > "$proof/elf-current.header" || return 1
  grep -Eq 'Class: +ELF64$' "$proof/elf-current.header" || return 1
  grep -Eq 'Machine: +Advanced Micro Devices X86-64$' "$proof/elf-current.header" || return 1
  grep -Eq 'Type: +(DYN|EXEC) ' "$proof/elf-current.header" || return 1
  readelf -l "$canonical" > "$proof/elf-current.program" || return 1
  readelf -d "$canonical" > "$proof/elf-current.dynamic" || return 1
  cat "$proof/elf-current.header" "$proof/elf-current.program" "$proof/elf-current.dynamic" >> "$proof/static-elfs.txt" || return 1
  interp=$(sed -n 's/.*Requesting program interpreter: \([^]]*\)\].*/\1/p' "$proof/elf-current.program") || return 1
  if [[ -n $interp ]]; then [[ $interp == /lib64/ld-linux-x86-64.so.2 ]] || return 1; hold "$interp" || return 1; fi
  # 재귀 호출 전에 dependency 목록을 값으로 보존한다.
  needed=$(sed -n 's/.*(NEEDED).*Shared library: \[\([^]]*\)\].*/\1/p' "$proof/elf-current.dynamic") || return 1
  while IFS= read -r candidate; do
    [[ -n $candidate ]] || continue
    [[ $candidate =~ ^[A-Za-z0-9_.+-]+$ ]] || return 1
    resolved=
    for root in "$sdk/lib" "$sdk/lib/rustlib/x86_64-unknown-linux-gnu/lib" /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /lib64; do
      if [[ -e $root/$candidate || -L $root/$candidate ]]; then
        path=$(readlink -e "$root/$candidate") || return 1
        [[ -z $resolved || $resolved == "$path" ]] || return 1
        resolved=$path
      fi
    done
    [[ -n $resolved ]] || return 1
    bind_elf "$resolved" || return 1
  done <<< "$needed"
}
pid_identity() {
  local line rest
  local -a fields
  IFS= read -r line < "/proc/$1/stat" || return 1
  [[ ${line%% *} == "$1" ]] || return 1
  rest=${line##*) }; read -r -a fields <<< "$rest"
  [[ ${#fields[@]} -ge 20 && ${fields[0]} != Z && ${fields[1]} == "$$" && ${fields[2]} == "$1" && ${fields[3]} == "$1" ]] || return 1
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
finish() {
  local original=$? status=0 n sealed=false held_code tree_code roster_code hash_code
  local tree_status=0 source_policy_code resource_code stop_code=0
  trap - EXIT INT TERM HUP
  set +e
  if [[ -n $active ]]; then kill_owned || status=1; wait "$active"; group_empty "$active" || status=1; active=; fi
  if [[ -n $watcher ]]; then : > "$stop"; wait "$watcher" || status=1; fi
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr"
  held_code=$?
  ((held_code == 0)) || status=1
  : > "$proof/final-tree-status.tsv" || status=1
  for n in "${!tree_paths[@]}"; do
    tree "${tree_paths[$n]}" "$proof/${tree_names[$n]}.after" > "$proof/final-tree-${tree_names[$n]}.stdout" 2> "$proof/final-tree-${tree_names[$n]}.stderr"
    tree_code=$?; roster_code=not-reached; hash_code=not-reached
    if ((tree_code == 0)); then
      cmp "$proof/${tree_names[$n]}.before.roster" "$proof/${tree_names[$n]}.after.roster" > "$proof/final-roster-${tree_names[$n]}.stdout" 2> "$proof/final-roster-${tree_names[$n]}.stderr"
      roster_code=$?
      if ((roster_code == 0)); then
        cmp "$proof/${tree_names[$n]}.before.sha256" "$proof/${tree_names[$n]}.after.sha256" > "$proof/final-hash-${tree_names[$n]}.stdout" 2> "$proof/final-hash-${tree_names[$n]}.stderr"
        hash_code=$?
      fi
    fi
    [[ $tree_code == 0 && $roster_code == 0 && $hash_code == 0 ]] || { tree_status=1; status=1; }
    printf '%s\t%s\t%s\t%s\n' "${tree_names[$n]}" "$tree_code" "$roster_code" "$hash_code" >> "$proof/final-tree-status.tsv" || status=1
  done
  originals > "$proof/source-policy-final.stdout" 2> "$proof/source-policy-final.stderr"
  source_policy_code=$?
  ((source_policy_code == 0)) || status=1
  admit > "$proof/resource-final.stdout" 2> "$proof/resource-final.stderr"
  resource_code=$?
  ((resource_code == 0)) || status=1
  [[ ! -e $job/proof/c8-resource-stop && ! -L $job/proof/c8-resource-stop ]] || { stop_code=1; status=1; }
  [[ $baseline == true && $status == 0 ]] && sealed=true
  jq -n --arg source_policy "$source_policy" --arg source "$GITHUB_SHA" --arg phase "$phase" --argjson code "$original" --argjson final "$status" \
    --argjson sealed "$sealed" --argjson pass "$passed" --argjson held "$held_code" --argjson trees "$tree_status" \
    --argjson source_policy_final "$source_policy_code" --argjson resource "$resource_code" --argjson stop "$stop_code" \
    '{schema:1,source_policy:$source_policy,source_commit:$source,phase:$phase,command_exit:$code,finalizer_exit:$final,immutable_seals:$sealed,
      held_final_exit_code:$held,tree_final_exit_code:$trees,source_policy_final_exit_code:$source_policy_final,resource_final_exit_code:$resource,resource_stop_exit_code:$stop,
      collision8_current_pass:($code==0 and $final==0 and $sealed and $pass),logical_cases:(if $pass then 8 else 0 end),
      original_cpp_executions:(if $pass then 0 else null end),probe_executions:(if $pass then 0 else null end),historical_pass_inherited:false,old27_admitted:false,
      runtime2_admitted:false,caller_admitted:false,source_absence_admitted:false,retirement_credit:0,
      closed_pid:false,closed_cwd:false,closed_fd:false,closed_dso:false,complete_build_dependency_closure:false,
      trace_scope:"checker8 exec/argv/env/terminal/spawn attempts; not a closed-family or hostile same-UID proof",
      recovery_scope:"direct owned leader waited and group checked; strace EXITKILL, not an all-adopted-reap claim",
      measured_peak_guarantee:false}' > "$proof/outcome.json" || status=1
  printf 'immutable_seals=%s\n' "$sealed" >> "$GITHUB_OUTPUT" || status=1
  [[ $original == 0 && $status == 0 && $passed == true ]] || exit 1
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
build_env=(HOME="$job/home" PATH="$sdk/bin:/usr/bin:/bin" TMPDIR="$build/tmp" LC_ALL=C TZ=UTC
  CARGO_HOME="$build/cargo-home" CARGO_TARGET_DIR="$build/target" RUSTUP_HOME="$job/rustup-home" RUSTUP_TOOLCHAIN=1.94.0
  RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TERM_COLOR=never
  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1)
runtime_env=(HOME="$job/home" PATH="$sdk/bin:/usr/bin:/bin" TMPDIR="$build/tmp" LC_ALL=C TZ=UTC)
# 명령 전체에 독립 PGID/상한. wrapper shell이 만든 PWD/SHLVL/_도 두 번째 env-i에서 제거한다.
run() {
  local label=$1 seconds=$2 trace_dir=$3 prefix=$4 profile=$5 env_kind=$6 code=0 watch_code=0 group_code=0 leader ready go stamp count
  local seccomp_requested=false
  local -a selected_env payload
  shift 6
  phase=$label
  admit
  [[ ! -e $job/proof/c8-resource-stop ]]
  selected_env=("${build_env[@]}")
  [[ $env_kind != runtime ]] || selected_env=("${runtime_env[@]}" "${case_env[@]}")
  payload=(/usr/bin/env -i "${selected_env[@]}" /usr/bin/timeout --foreground --signal=TERM --kill-after=30s "${seconds}s")
  if [[ $trace_dir != none ]]; then
    printf '%s\0' "$@" | jq -Rs 'split("\u0000")[:-1]' > "$proof/build/$label.argv.expected.json"
    printf '%s\n' "${selected_env[@]}" | jq -Rs 'split("\n")[:-1]' > "$proof/build/$label.env.expected.json"
    payload+=(/usr/bin/strace)
    # dd6157 build의 짧은 clone3 thread에서 ??? 미완료 기록이 관측됐다.
    # syscall 선택/lexer는 그대로 두고 build capture만 기존 retained 방식으로 요청한다.
    if [[ $env_kind == build ]]; then payload+=(--seccomp-bpf); seccomp_requested=true; fi
    payload+=(--kill-on-exit -ff -ttt -yy -xx -v -s 65536 -e "trace=$profile" -o "$trace_dir/$prefix")
    jq -n --arg label "$label" --arg prefix "$prefix" --arg profile "$profile" --argjson seccomp "$seccomp_requested" \
      '{schema:1,label:$label,prefix:$prefix,trace_profile:$profile,seccomp_bpf_requested:$seccomp,seccomp_bpf_activation_proven:false,unknown_trace_accepted:false,retirement_credit:0}' > "$proof/build/$label.capture.json"
  fi
  payload+=("$@")
  ready=$proof/build/owned-$label.ready; go=$proof/build/owned-$label.go; stop=$proof/build/owned-$label.stop
  [[ ! -e $ready && ! -e $go && ! -e $stop ]]
  printf '%s\0' "$label" "$@" >> "$proof/commands.nul"
  /usr/bin/env -i HOME="$job/home" PATH=/usr/bin:/bin LC_ALL=C /usr/bin/setsid /usr/bin/bash --noprofile --norc -c '
    ulimit -c 0; ulimit -f 524288
    ready=$1; go=$2; shift 2
    printf "%s\n" "$$" > "$ready"
    for attempt in {1..300}; do
      [[ ! -e $go ]] || break
      /usr/bin/sleep 0.01
    done
    [[ -e $go ]] || exit 125
    exec "$@"
  ' collision8-owned "$ready" "$go" "${payload[@]}" > "$proof/build/$label.stdout" 2> "$proof/build/$label.stderr" < /dev/null &
  leader=$!; active=$leader; active_identity=
  for count in {1..300}; do
    if [[ -f $ready && $(cat "$ready") == "$leader" ]]; then active_identity=$(pid_identity "$leader") && break; fi
    sleep 0.01
  done
  [[ -n $active_identity ]] || fail 'owned PGID handshake failed'
  : > "$go"
  (
    scan_role=watcher
    while [[ ! -e $stop ]] && [[ $(pid_identity "$leader") == "$active_identity" ]]; do
      if ! admit; then
        printf '%s\n' "$label" > "$job/proof/c8-resource-stop"
        kill_owned || true
        exit 1
      fi
      sleep 2
    done
  ) &
  watcher=$!
  wait "$leader" || code=$?
  # wait 이후의 과거 PGID에는 신호하지 않는다. watcher도 현재 starttime/소유성 확인 뒤에만 신호한다.
  : > "$stop"
  wait "$watcher" || watch_code=$?
  active= active_identity= watcher=
  group_empty "$leader" || group_code=$?
  printf '%s\t%s\t%s\n' "$label" "$code" "$watch_code" >> "$proof/exit-codes.tsv"
  if ((code != 0 || watch_code != 0 || group_code != 0)); then
    # 최초 command 실패는 그대로 남긴다. finalizer/후속 진단이 덮어쓰지 않는다.
    (set -C; printf 'phase=%s\ncommand_exit=%s\nwatcher_exit=%s\ngroup_exit=%s\nstderr=%s\n' \
      "$label" "$code" "$watch_code" "$group_code" "$proof/build/$label.stderr" > "$proof/command-first-failure.txt") || return 1
    printf 'logging collision8 failed: phase=%s command=%s watcher=%s group=%s; stderr prefix (max4096B):\n' "$label" "$code" "$watch_code" "$group_code" >&2
    head -c 4096 -- "$proof/build/$label.stderr" >&2 || return 1
    return 1
  fi
  admit
  [[ ! -e $job/proof/c8-resource-stop ]]
}
artifact() {
  local log=$1 package=$2 name=$3 kind=$4 src=$5 tested=$6
  jq -ser --arg package "path+file://$source/rust/crates/$package#openpilot-$package@0.1.0" \
    --arg manifest "$source/rust/crates/$package/Cargo.toml" --arg name "$name" --arg kind "$kind" \
    --arg src "$src" --argjson tested "$tested" '
    if length<1 or .[-1].reason!="build-finished" or .[-1].success!=true or
      ([.[]|select(.reason=="build-finished")]|length)!=1 or
      (all(.[]; type=="object" and (.reason=="compiler-artifact" or .reason=="compiler-message" or
        .reason=="build-script-executed" or .reason=="build-finished"))|not)
    then error("Cargo EOF or row differs") else . end |
    [.[]|select(.reason=="compiler-artifact" and .package_id==$package and .manifest_path==$manifest and
      .target.name==$name and .target.kind==[$kind] and .target.src_path==$src and .profile.test==$tested and .executable!=null)] |
    if length==1 and (.[0].executable|type)=="string" and (.[0].executable|startswith("/"))
    then .[0].executable else error("unique current executable required") end' "$log"
}
for tool in bash sh git find sort xargs sha256sum stat readlink readelf sed awk grep head od tr cat cmp df du dd jq strace timeout setsid sleep curl c++ cc ar as ld ranlib nm capnp pkg-config dirname basename cut mkdir chmod ln rm id; do
  bind_elf "$(command -v "$tool")"
done
for tool in cargo rustc rustdoc; do bind_elf "$sdk/bin/$tool"; done
bind_elf "$job/cargo-home/bin/rustup"
bind_elf "$job/rust-bootstrap/rustup-init"
hold /etc/ld.so.cache
hold "$source/tools/ci/logging_collision8_549.sh"
if [[ $source_policy == absent ]]; then hold "$source_policy_baseline"; hold "$source_policy_metadata"; fi
hold "$batch_worker"
hold "$CI_PREPARED_RUST_MANIFEST"
hold "$source/.github/workflows/python-rust-523-logging-collision8.yml"
for relative in openpilot/common/swaglog.cc openpilot/common/swaglog.h openpilot/common/timing.h rust/tools/native_logging_reference.cc; do hold "$source/$relative"; done
originals
add_tree source "$source"
add_tree system-headers /usr/include
cxx=$(readlink -e "$(command -v c++)")
# 메타데이터 질의도 인증 없는 환경에서 수행한다. target/ref는 실행하지 않는다.
declare -a cpp_exec_paths=("$cxx" "$(command -v c++)" "$(command -v curl)" "$(command -v as)" "$(command -v ld)")
for name in cc1plus collect2 as ld; do
  queried=$(/usr/bin/env -i "${build_env[@]}" /usr/bin/timeout --signal=TERM --kill-after=5s 30s "$cxx" "-print-prog-name=$name")
  printf '%s\t%s\n' "$name" "$queried" >> "$proof/compiler-tools.tsv"
  [[ $queried == /* ]] || queried=$(command -v "$queried")
  [[ $queried == /* && -f $queried ]]
  bind_elf "$queried"
  cpp_exec_paths+=("$queried" "$(readlink -e "$queried")")
  [[ $name != cc1plus ]] || cc1=$queried
done
for name in Scrt1.o crti.o crtn.o crtbeginS.o crtendS.o libstdc++.so libgcc.a libgcc_s.so libc.so libc_nonshared.a libpthread.a; do
  queried=$(/usr/bin/env -i "${build_env[@]}" /usr/bin/timeout --signal=TERM --kill-after=5s 30s "$cxx" "-print-file-name=$name")
  [[ $queried == /* && -f $queried ]]
  printf '%s\t%s\n' "$name" "$queried" >> "$proof/compiler-link-data.tsv"
  hold "$queried"
done
bind_elf /usr/lib/x86_64-linux-gnu/libzmq.so.5
/usr/bin/env -i "${build_env[@]}" /usr/bin/timeout --signal=TERM --kill-after=5s 30s "$cxx" --version > "$proof/cxx-version.txt"
/usr/bin/env -i "${build_env[@]}" /usr/bin/timeout --signal=TERM --kill-after=5s 30s "$cxx" -print-search-dirs > "$proof/cxx-search-dirs.txt"
/usr/bin/env -i "${build_env[@]}" /usr/bin/timeout --signal=TERM --kill-after=5s 30s /usr/bin/pkg-config --cflags --libs libzmq > "$proof/zmq-pkg-config.txt"
gcc_root=$(dirname "$cc1")
add_tree gcc-provider "$gcc_root"
baseline=true
case_env=()
printf '%s  %s\n' 81bc4b7a6f9541f61a27d96ffb2edbcdfe1ab1ea3a4b97f1dc48725e7a0a0a44 "$source/rust/crates/dev-tools/src/native_logging_build.rs" \
  9660eaa99116198dc5471b55d8048b3d19a9997568d1806aa6fe8bdac81d5469 "$source/rust/crates/dev-tools/src/json11_stage.rs" \
  5a029df81ef9216c9d5d920c6747b7c01d1cb238245bd4fd3b8a3041510aced0 "$source/rust/crates/dev-tools/tests/native_logging_lifecycle.rs" \
  3d3a7e31e04e654a86ed332fb8b97d67f5648905ee0b276bc53cfbc87fe5dcdf "$source/uv.lock" \
  | sha256sum --check --strict --quiet -
run strace-help 30 none unused unused build /usr/bin/strace --help
grep -F -- --kill-on-exit "$proof/build/strace-help.stdout" > "$proof/exitkill-supported.txt"
grep -F -- --seccomp-bpf "$proof/build/strace-help.stdout" > "$proof/seccomp-bpf-supported.txt"
common=(--manifest-path "$source/rust/Cargo.toml" -p openpilot-dev-tools --locked --no-default-features --features native-logging-check-native)
run checker-build 1800 "$proof/cargo-trace" checker process build "$sdk/bin/cargo" build "${common[@]}" --bin check-native-logging --bin build-native-logging-reference --message-format=json
run probe-build 1800 "$proof/cargo-trace" probe process build "$sdk/bin/cargo" build --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-logging --example native_logging_probe --message-format=json
run fixture-build 1800 "$proof/cargo-trace" fixture process build "$sdk/bin/cargo" test "${common[@]}" --test native_logging_lifecycle --no-run --message-format=json
checker=$(artifact "$proof/build/checker-build.stdout" dev-tools check-native-logging bin "$source/rust/crates/dev-tools/src/bin/check_native_logging.rs" false)
builder=$(artifact "$proof/build/checker-build.stdout" dev-tools build-native-logging-reference bin "$source/rust/crates/dev-tools/src/bin/build_native_logging_reference.rs" false)
probe=$(artifact "$proof/build/probe-build.stdout" logging native_logging_probe example "$source/rust/crates/logging/examples/native_logging_probe.rs" false)
fixture=$(artifact "$proof/build/fixture-build.stdout" dev-tools native_logging_lifecycle test "$source/rust/crates/dev-tools/tests/native_logging_lifecycle.rs" true)
[[ $checker == "$build/target/debug/check-native-logging" ]]
for path in "$checker" "$builder" "$probe" "$fixture"; do
  [[ $path == "$build/target/"* && $(readlink -e "$path") == "$path" && -x $path && ! -L $path ]]
  bind_elf "$path"
done
jq -n --arg checker "$checker" --arg builder "$builder" --arg probe "$probe" --arg fixture "$fixture" \
  '{checker:$checker,builder:$builder,probe:$probe,fixture:$fixture}' > "$proof/current-artifacts.json"
[[ -f /usr/include/zmq.h && ! -L /usr/include/zmq.h ]]
hold /usr/include/zmq.h
declare -A cpp_allowed_ids=()
for path in "$builder" "${cpp_exec_paths[@]}"; do
  canonical=$(readlink -e "$path")
  [[ -v held_id[$canonical] ]]
  cpp_allowed_ids[$canonical]=${held_id[$canonical]}
done
[[ ! -e $proof/reference-build && ! -L $proof/reference-build ]]
run reference-build 660 "$proof/reference-trace" reference 'process,%file,%memory,setsid,setpgid,prctl,unshare,setns' build \
  "$builder" --root "$source" --output "$proof/reference-build" --zmq-include /usr/include --architecture x86_64
fresh_reference=$proof/reference-build/native_logging_reference
printf '%s\n' "$fresh_reference" > "$proof/reference-path.expected"
cmp "$proof/reference-path.expected" "$proof/build/reference-build.stdout"
wheel=$proof/reference-build/comma_deps_json11-20170411.0.post103-py3-none-manylinux_2_28_x86_64.whl
[[ $(stat -c %s "$wheel") == 42604 ]]
printf '%s  %s\n' a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83 "$wheel" | sha256sum --check --strict --quiet -
hold "$wheel"
bind_elf "$fresh_reference"
add_tree reference "$proof/reference-build"
find -P "$proof/reference-build/json11" -mindepth 1 -maxdepth 1 -type d -name '*.data' -print0 > "$proof/json11-prefixes.bin"
mapfile -d '' -t prefixes < "$proof/json11-prefixes.bin"
[[ ${#prefixes[@]} == 1 ]]
json11_prefix=${prefixes[0]}/purelib/json11/install
[[ -f $json11_prefix/lib/libjson11.a && -d $json11_prefix/include ]]
jq -n --arg source "$source" --arg output "$proof/reference-build" --arg json11 "$json11_prefix" \
  ' ["c++","-std=c++17","-O2","-pthread","-I"+$output+"/overlay","-I"+$source+"/openpilot",
    "-I"+$json11+"/include","-I/usr/include",$source+"/rust/tools/native_logging_reference.cc",
    $source+"/openpilot/common/swaglog.cc",$json11+"/lib/libjson11.a","-l:libzmq.so.5",
    "-Wl,--wrap=zmq_send","-Wl,--wrap=zmq_setsockopt","-o",$output+"/native_logging_reference"]' > "$proof/reference-command.expected.json"
jq -e --slurpfile command "$proof/reference-command.expected.json" '
  .json11_version=="20170411.0.post103" and .json11_architecture=="x86_64" and
  .json11_sha256=="a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83" and .command==$command[0]
  ' "$proof/reference-build/provenance.json" > "$proof/reference-provenance-check.txt"
: > "$proof/reference-sources.expected.jsonl"
for relative in openpilot/common/swaglog.cc openpilot/common/swaglog.h openpilot/common/timing.h rust/tools/native_logging_reference.cc; do
  jq -cn --arg path "$relative" --arg sha "${held_sha[$source/$relative]}" '{key:$path,value:$sha}' >> "$proof/reference-sources.expected.jsonl"
done
jq -e --slurpfile rows "$proof/reference-sources.expected.jsonl" '.sources==($rows|from_entries)' "$proof/reference-build/provenance.json" > "$proof/reference-sources.check"
# 현재 fixture에는 process_group/pre_exec가 없다. 알려진 setup 추가는 별도 source 심사 대상이다.
selector=actual_cli_foreign_output_collisions_precede_builder_or_provider_work
case_env=(NATIVE_LOGGING_TEST_BINARY="$probe" NATIVE_LOGGING_TEST_BUILDER="$builder" NATIVE_LOGGING_TEST_REFERENCE="$fresh_reference"
  NATIVE_LOGGING_LIFECYCLE_EVIDENCE="$proof/cases" NATIVE_LOGGING_LIFECYCLE_RESUME=39833849895 "${source_policy_env[@]}")
socket_max=$proof/cases/logging-lifecycle-XXXXXX/output/s
(( ${#socket_max} < 108 ))
printf '%s\n' "${runtime_env[@]}" "${case_env[@]}" | jq -Rs 'split("\n")[:-1]' > "$proof/fixture-env.expected.json"
started=$(cut -d ' ' -f 1 /proc/uptime)
run collision8 600 "$proof/case-trace" collision 'process,%file,%memory,setsid,setpgid,prctl,unshare,setns' runtime \
  "$fixture" --ignored --exact "$selector" --test-threads=1 --color=never
ended=$(cut -d ' ' -f 1 /proc/uptime)
awk -v start="$started" -v end="$ended" 'BEGIN {exit !(start~/^[0-9]+[.][0-9]+$/ && end~/^[0-9]+[.][0-9]+$/ && end>=start && end-start<=600)}'
[[ $(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$proof/build/collision8.stdout") == 1 ]]
[[ $(grep -Fxc "test $selector ... ok" "$proof/build/collision8.stdout") == 1 ]]
add_tree cases "$proof/cases"
find -P "$proof/cases" -mindepth 1 -maxdepth 1 -printf '%f\t%y\n' > "$proof/case-entries.tsv"
entries=0
while IFS=$'\t' read -r name kind; do
  [[ $name =~ ^logging-lifecycle-[A-Za-z0-9]{6}$ && $kind == d ]]
  entries=$((entries+1))
done < "$proof/case-entries.tsv"
[[ $entries == 8 ]]
find "$proof/cases" -mindepth 1 -maxdepth 1 -type d -name 'logging-lifecycle-*' -print0 > "$proof/case-dirs.bin"
mapfile -d '' -t case_dirs < "$proof/case-dirs.bin"
[[ ${#case_dirs[@]} == 8 ]]
printf '%s\n' '[[false,"live-directory"],[false,"symlink"],[false,"dangling-symlink"],[true,"file"],[true,"directory"],[true,"live-directory"],[true,"symlink"],[true,"dangling-symlink"]]' > "$proof/case-roster.expected.json"
: > "$proof/case-manifest.jsonl"
for path in "${case_dirs[@]}"; do
  [[ $path == "$proof/cases/logging-lifecycle-"?????? && ! -L $path && $(readlink -e "$path") == "$path" ]]
  receipt=$path/collision-case.json
  [[ -f $receipt && ! -L $receipt && $(stat -c %s "$receipt") -le 1048576 ]]
  hold "$receipt"
  [[ ! -e $path/unexpected-child && ! -L $path/unexpected-child &&
     -f $path/checker.stdout && ! -L $path/checker.stdout && ! -s $path/checker.stdout &&
     -f $path/checker.stderr && ! -L $path/checker.stderr ]]
  grep -F 'logging evidence output already exists' "$path/checker.stderr" > /dev/null
  jq -se --arg root "$path" '
    length==1 and (.[0]|keys)==["after","before","kind","prebuilt","producer_barrier_absent","returncode"] and
    (.[0]|.returncode==1 and .producer_barrier_absent==true and (.prebuilt|type)=="boolean" and
      (.kind|type)=="string" and (.before|keys)==["external","output"] and .before==.after)
    ' "$receipt" > "$proof/case-$(basename "$path").check"
  jq -c --arg root "$path" '{root:$root,prebuilt,kind}' "$receipt" >> "$proof/case-manifest.jsonl"
done
jq -se --slurpfile expected "$proof/case-roster.expected.json" \
  'length==8 and ([.[]|[.prebuilt,.kind]]|sort)==($expected[0]|sort)' "$proof/case-manifest.jsonl" > "$proof/case-roster.check"
run inspect-collision 180 none unused unused build "$checker" inspect-trace --directory "$proof/case-trace" --prefix collision --output "$proof/inspect/collision.json"
# raw trace에서 expected를 만들지 않는다. 현재 source의 argv/env recipe를 case root별로 독립 생성한다.
jq -n --arg source_policy "$source_policy" --arg source "$source" --arg checker "$checker" --arg probe "$probe" --arg builder "$builder" --arg ref "$fresh_reference" \
  --slurpfile cases "$proof/case-manifest.jsonl" --slurpfile env "$proof/fixture-env.expected.json" '
  [$cases[]|{root:.root,key:[.prebuilt,.kind],program:$checker,
    argv:([$checker,"--binary",$probe,"--builder",$builder,"--root",$source+"/rust/crates/dev-tools/../../..","--output",.root+"/output"]+
      (if .prebuilt then ["--reference",$ref] else [] end)+
      (if $source_policy=="absent" then ["--source-policy","absent"] else [] end)),
    env:($env[0]+["NATIVE_LOGGING_QA_HOLD=first","NATIVE_LOGGING_QA_RECORDS="+.root+"/unexpected-child"])}]' > "$proof/checkers.expected.json"
jq -e --arg fixture "$fixture" --arg selector "$selector" --arg checker "$checker" \
  --slurpfile wanted "$proof/checkers.expected.json" --slurpfile fixture_env "$proof/fixture-env.expected.json" \
  --slurpfile roster "$proof/case-roster.expected.json" '
  def bytes: explode;
  def vector: map(.bytes);
  def exactexec($argv;$env): .operation=="execve" and .outcome=="exec_succeeded" and .opaque_payload==false and
    (.exec.argv|vector)==($argv|map(bytes)) and .exec.environment.complete==true and
    .exec.environment.representation=="vector" and
    (.exec.environment.entries|vector|sort)==($env|map(bytes)|sort);
  . as $trace | .events as $events |
  [.events[]|select(.exec!=null)] as $execs |
  [$execs[]|select(.exec.program.bytes==($fixture|bytes))] as $roots |
  [$execs[]|select(.exec.program.bytes==($checker|bytes))] as $checks |
  .schema==1 and .raw_immutable==true and .requires_exit_status_zero==true and
  .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and
  .full_acceptance==false and .retirement_credit==false and
  (.raw_files|length)==(.directory_roster|length) and
  all(.directory_roster[]; .name|test("^collision[.][1-9][0-9]*$")) and
  ($execs|length)==9 and ($roots|length)==1 and ($checks|length)==8 and
  ($checks|map(.pid)|unique|length)==8 and
  ($roots[0]|exactexec([$fixture,"--ignored","--exact",$selector,"--test-threads=1","--color=never"];$fixture_env[0])) and
  ([$events[]|select(.pid==$roots[0].pid and (.kind|startswith("terminal_")))]|
    length==1 and .[0].kind=="terminal_exit" and .[0].result==("+++ exited with 0 +++"|bytes)) and
  ([$checks[]|. as $check|
    [$wanted[0][]|. as $want|select($check|exactexec($want.argv;$want.env))] as $matches |
    if ($matches|length)!=1 then error("case argv/env differs") else
      {key:$matches[0].key,time:($check.source[0].timestamp|tonumber)} end]|sort_by(.time)|map(.key))==$roster[0] and
  all($checks[]; . as $check |
    ([$events[]|select(.pid==$check.pid and (.kind|startswith("terminal_")))]|
      length==1 and .[0].kind=="terminal_exit" and .[0].result==("+++ exited with 1 +++"|bytes)) and
    all($events[]|select(.pid==$check.pid);
      (.operation!="fork" and .operation!="vfork" and .operation!="clone" and .operation!="clone3") and
      # pre-exec harness와 post-exec runtime을 구분한다. 고정 fixture에는 session hook 자체가 없다.
      (if .source[0].line<$check.source[0].line
       then .operation!="setpgid" and .operation!="setsid" and .operation!="unshare" and .operation!="setns"
       else .operation!="setpgid" and .operation!="setsid" and .operation!="unshare" and .operation!="setns" end)))
  ' "$proof/inspect/collision.json" > "$proof/collision-policy.check"
# build에서 reference/probe 실행0도 같은 표준 byte decoder로 확인한다. compiler 실행과 섞어 case allow하지 않는다.
printf '%s\n' "$builder" "${cpp_exec_paths[@]}" | jq -Rs 'split("\n")[:-1]|unique' > "$proof/cpp-programs.expected.json"
# 성공한 build exec의 현재 파일도 묶는다. 상대경로/삭제된 실행파일/loader 직접기동은 추측하지 않는다.
# 이 사후 파일 관측은 exec 시점의 완전한 PID/cwd/DSO closure라는 뜻이 아니다.
audit_build_programs() {
  local report=$1 prefix=$2 path canonical name
  jq -r '.events[]|select(.exec!=null and .outcome=="exec_succeeded")|.exec.program.bytes|implode' "$report" > "$proof/build-programs.current.txt"
  if [[ ! -s $proof/build-programs.current.txt ]]; then
    [[ $prefix == fixture-batch ]] || return 1
    jq -e '[.events[]|select(.exec!=null and .outcome=="exec_succeeded")]|length==0' "$report" > /dev/null || return 1
  fi
  while IFS= read -r path; do
    [[ $path =~ ^/[A-Za-z0-9_./+-]+$ ]] || return 1
    name=${path##*/}
    case "$name" in python*|pypy*|ld-linux*|check_native_logging.py|native_logging_transport.py) return 1 ;; esac
    canonical=$(readlink -e "$path") || return 1
    hold "$path" || return 1
    if [[ $prefix == reference ]]; then
      [[ -v cpp_allowed_ids[$canonical] && ${cpp_allowed_ids[$canonical]} == "${held_id[$canonical]}" ]] || return 1
    fi
    [[ ${held_sha[$canonical]} != "${held_sha[$fresh_reference]}" &&
       ${held_sha[$canonical]} != "${held_sha[$probe]}" ]] || return 1
  done < "$proof/build-programs.current.txt"
}
fixture_batches() {
  local started now remaining batch files bytes identity digest report report_fd actual
  local path name expected index=$proof/fixture-batches
  local -a context
  IFS=' ' read -r started _ < /proc/uptime
  add_tree cargo-raw "$proof/cargo-trace"
  hold "$proof/current-artifacts.json"
  context=(HOME="$job/home" PATH=/usr/bin:/bin TMPDIR="$build/tmp" LC_ALL=C TZ=UTC
    CARGO_HOME="$build/cargo-home" CARGO_TARGET_DIR="$build/target" RUSTUP_HOME="$job/rustup-home"
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1
    GITHUB_ACTIONS=true GITHUB_REPOSITORY="$GITHUB_REPOSITORY" GITHUB_EVENT_NAME="$GITHUB_EVENT_NAME"
    GITHUB_REF="$GITHUB_REF" GITHUB_SHA="$GITHUB_SHA" GITHUB_RUN_ID="$GITHUB_RUN_ID"
    GITHUB_RUN_ATTEMPT="$GITHUB_RUN_ATTEMPT" GITHUB_JOB="$GITHUB_JOB" GITHUB_WORKSPACE="$source"
    RUNNER_ENVIRONMENT="$RUNNER_ENVIRONMENT" RUNNER_OS="$RUNNER_OS" RUNNER_ARCH="$RUNNER_ARCH"
    DIAG_SOURCE="$source" CI_JOB_TEMP="$job" DIAG_SOURCE_OWNER="$owner" DIAG_ACTIONS_WORKSPACE="$workspace"
    DIAG_SOURCE_IDENTITY="$DIAG_SOURCE_IDENTITY" CI_PREPARED_RUST_ROOT="$sdk" CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST"
    COLLISION8_BATCHES_SHA256="$COLLISION8_BATCHES_SHA256" COLLISION8_CHECKER_SHA256="${held_sha[$checker]}"
    COLLISION8_SDK_MANIFEST_SHA256="${held_sha[$CI_PREPARED_RUST_MANIFEST]}"
    COLLISION8_ARTIFACTS_SHA256="${held_sha[$proof/current-artifacts.json]}")
  IFS=' ' read -r now _ < /proc/uptime
  remaining=$(awk -v start="$started" -v now="$now" 'BEGIN{r=int(180-(now-start));if(r<1||r>180)exit 1;print r}')
  # 전체 worker에 한 번만 owned timeout/watcher를 둔다. batch마다180s를 새로 주지 않는다.
  run inspect-fixture-build "$remaining" none unused unused build /usr/bin/env -i "${context[@]}" /usr/bin/bash --noprofile --norc "$batch_worker"
  for path in worker-index.json original.jsonl coverage.jsonl batches.jsonl; do hold "$index/$path"; done
  jq -e --arg source "$GITHUB_SHA" --arg worker "$COLLISION8_BATCHES_SHA256" --arg checker "${held_sha[$checker]}" '
    .schema==1 and .source_commit==$source and .worker_sha256==$worker and .checker_sha256==$checker and
    (.files|type)=="number" and .files>0 and .files<=8192 and .raw_bytes>0 and .raw_bytes<=1073741824 and
    .batches>0 and .batches<=.files' "$index/worker-index.json" > "$index/index.check"
  # 작은 manifest만 slurp한다. full Event 배열은 각 report를 처리하고 버린다.
  local coverage_policy='
    . as $coverage | $original as $originals | $batches as $batches |
    length==$index[0].files and length>0 and
    (map(.name)|unique|length)==length and (map(.pid)|unique|length)==length and
    (map({name,pid,identity:.original_identity,sha256})|sort_by(.name))==($originals|sort_by(.name)) and
    ($batches|length)==$index[0].batches and ($batches|map(.batch)|unique|length)==($batches|length) and
    all($coverage[]; (.batch|test("^[0-9]{4}$")) and (.name|test("^fixture[.][1-9][0-9]*$")) and
      .pid>0 and .pid<=4294967295 and .copy_identity.links==1 and .copy_identity.size==.original_identity.size and
      (.copy_identity.dev!=.original_identity.dev or .copy_identity.ino!=.original_identity.ino) and
      (.copy_stat|type)=="string") and
    all($batches[]; . as $batch |
      [$coverage[]|select(.batch==$batch.batch)] as $rows |
      $batch.files==($rows|length) and $batch.files>0 and $batch.files<=128 and
      $batch.raw_bytes==($rows|map(.original_identity.size)|add) and $batch.raw_bytes<=1048576) and
    all($coverage[]; . as $row | any($batches[]; .batch==$row.batch)) and
    (map(.original_identity.size)|add)==$index[0].raw_bytes'
  check_coverage() {
    jq -se --slurpfile original "$index/original.jsonl" --slurpfile batches "$index/batches.jsonl" \
      --slurpfile index "$index/worker-index.json" "$coverage_policy" "$1"
  }
  check_coverage "$index/coverage.jsonl" > "$index/coverage.check"
  # 실제 valid 입력에서 한 요소만 바꾼 coverage 통제다. exit1+false를 모두 요구한다.
  jq -sc '.[1:][]' "$index/coverage.jsonl" > "$index/coverage-missing.jsonl"
  jq -sc '(.+.[0:1])[]' "$index/coverage.jsonl" > "$index/coverage-duplicate.jsonl"
  for path in "$index/coverage-missing.jsonl" "$index/coverage-duplicate.jsonl"; do
    local control_code=0
    check_coverage "$path" > "$path.check" || control_code=$?
    [[ $control_code == 1 && $(cat "$path.check") == false ]] || fail 'coverage negative failed'
  done
  printf '%s\n' 'positive+missing+duplicate verified' > "$index/coverage-controls.txt"
  find -P "$index/raw" -mindepth 1 -maxdepth 1 -printf '%f\t%y\n' | sort > "$index/directories.actual.tsv"
  jq -rs 'sort_by(.batch)[]|.batch+"\td"' "$index/batches.jsonl" > "$index/directories.expected.tsv"
  cmp "$index/directories.actual.tsv" "$index/directories.expected.tsv"
  mkdir "$index/checks"
  jq -r '[.batch,.files,.raw_bytes,.report_identity,.report_sha256]|@tsv' "$index/batches.jsonl" > "$index/reports.tsv"
  while IFS=$'\t' read -r batch files bytes identity digest; do
    IFS=' ' read -r now _ < /proc/uptime
    awk -v start="$started" -v now="$now" 'BEGIN{exit !(now>=start && now-start<=180)}'
    report=$index/reports/$batch.json
    [[ $batch =~ ^[0-9]{4}$ && -f $report && ! -L $report && $(idfile "$report") == "$identity" ]]
    exec {report_fd}<"$report"
    [[ $(idfile "/proc/$$/fd/$report_fd") == "$identity" ]]
    actual=$(sha256sum "/proc/$$/fd/$report_fd"); [[ ${actual%% *} == "$digest" ]]
    jq -se --arg batch "$batch" '[.[]|select(.batch==$batch)]' "$index/coverage.jsonl" > "$index/checks/$batch.inputs.json"
    jq -e --arg directory "$index/raw/$batch" --arg ref "$fresh_reference" --arg probe "$probe" \
      --slurpfile wanted "$index/checks/$batch.inputs.json" \
      --slurpfile argv "$proof/build/fixture-build.argv.expected.json" --slurpfile env "$proof/build/fixture-build.env.expected.json" '
      . as $report | [.events[]|select(.exec!=null)] as $execs |
      [$execs[]|select(.exec.program.bytes==($argv[0][0]|explode))] as $roots |
      [.events[]|select(.kind|startswith("terminal_"))] as $terminals |
      if .schema==1 and .scope=="read-only rich strace lexical inspection; no execution policy or family admission" and
        .directory==$directory and .prefix=="fixture" and .raw_immutable==true and .requires_exit_status_zero==true and
        .full_acceptance==false and .retirement_credit==false and .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and
        (.directory_roster|sort_by(.name))==($wanted[0]|map({name,identity:.copy_identity})|sort_by(.name)) and
        (.raw_files|map({name,pid,identity_before,identity_after,sha256})|sort_by(.name))==
          ($wanted[0]|map({name,pid,identity_before:.copy_identity,identity_after:.copy_identity,sha256})|sort_by(.name)) and
        .selected_raw_bytes==($wanted[0]|map(.original_identity.size)|add) and
        ($roots|length)<=1 and all($roots[]; .operation=="execve" and .outcome=="exec_succeeded" and
          .opaque_payload==false and (.exec.argv|map(.bytes))==($argv[0]|map(explode)) and
          .exec.environment.complete==true and .exec.environment.representation=="vector" and
          (.exec.environment.entries|map(.bytes)|sort)==($env[0]|map(explode)|sort)) and
        all($execs[]; .exec.program.bytes!=($ref|explode) and .exec.program.bytes!=($probe|explode)) and
        ($terminals|map(.pid)|sort)==($wanted[0]|map(.pid)|sort) and
        all($terminals[]; (.source|length)==1 and .source[0].line>0)
      then {batch:$wanted[0][0].batch,files:(.raw_files|length),raw_bytes:.selected_raw_bytes,
        roots:($roots|length),events:(.events|length),lines:($terminals|map(.source[0].line)|add),
        programs:([$execs[]|select(.outcome=="exec_succeeded")|.exec.program.bytes|implode|split("/")[-1]]|
          group_by(.)|map({name:.[0],count:length}))}
      else error("fixture batch full-schema policy differs") end' "/proc/$$/fd/$report_fd" > "$index/checks/$batch.json"
    audit_build_programs "/proc/$$/fd/$report_fd" fixture-batch
    actual=$(sha256sum "/proc/$$/fd/$report_fd")
    [[ ${actual%% *} == "$digest" && $(idfile "$report") == "$identity" && $(idfile "/proc/$$/fd/$report_fd") == "$identity" ]]
    exec {report_fd}<&-
  done < "$index/reports.tsv"
  # 생존 시점 단정 없이 원본/copy의 현재 identity와 전체 bytes를 다시 확인한다.
  jq -r '[.batch,.name,.copy_stat]|@tsv' "$index/coverage.jsonl" > "$index/copies.tsv"
  while IFS=$'\t' read -r batch name expected; do
    path=$index/raw/$batch/$name
    [[ -f $path && ! -L $path && $(idfile "$path") == "$expected" ]]
  done < "$index/copies.tsv"
  jq -r --arg root "$index/raw/" '.sha256+"  "+$root+.batch+"/"+.name' "$index/coverage.jsonl" > "$index/copies.sha256"
  sha256sum --check --strict --quiet "$index/copies.sha256"
  find -P "$index/raw" -mindepth 1 -printf '%P\t%y\n' | sort > "$index/copy-roster.actual.tsv"
  { cat "$index/directories.expected.tsv"; jq -r '.batch+"/"+.name+"\tf"' "$index/coverage.jsonl"; } | sort > "$index/copy-roster.expected.tsv"
  cmp "$index/copy-roster.actual.tsv" "$index/copy-roster.expected.tsv"
  tree "$proof/cargo-trace" "$proof/cargo-raw.checked"
  cmp "$proof/cargo-raw.before.roster" "$proof/cargo-raw.checked.roster"
  cmp "$proof/cargo-raw.before.sha256" "$proof/cargo-raw.checked.sha256"
  add_tree fixture-batch-inputs "$index/raw"
  add_tree fixture-batch-reports "$index/reports"
  jq -se --slurpfile wanted "$index/worker-index.json" '
    if length==$wanted[0].batches and (map(.batch)|unique|length)==length and
      (map(.files)|add)==$wanted[0].files and (map(.raw_bytes)|add)==$wanted[0].raw_bytes and
      (map(.roots)|add)==1 and (map(.events)|add)<=1000000 and (map(.lines)|add)<=2000000
    then {batch_count:length,files:(map(.files)|add),raw_bytes:(map(.raw_bytes)|add),root_count:1,
      events:(map(.events)|add),physical_lines:(map(.lines)|add),
      programs:([.[].programs[]]|group_by(.name)|map({name:.[0].name,count:(map(.count)|add)}))}
    else error("fixture batch aggregate differs") end' "$index"/checks/[0-9][0-9][0-9][0-9].json > "$index/aggregate.json"
  verify_held
  admit
  IFS=' ' read -r now _ < /proc/uptime
  awk -v start="$started" -v now="$now" 'BEGIN{exit !(now>=start && now-start<=180)}'
  hold "$index/aggregate.json"
  jq -n --arg source "$GITHUB_SHA" --arg worker "$COLLISION8_BATCHES_SHA256" \
    --arg coverage "${held_sha[$index/coverage.jsonl]}" --arg reports "${held_sha[$index/batches.jsonl]}" \
    --arg aggregate "${held_sha[$index/aggregate.json]}" --slurpfile counts "$index/aggregate.json" \
    '{schema:1,source_commit:$source,worker_sha256:$worker,coverage_sha256:$coverage,reports_index_sha256:$reports,
      aggregate_sha256:$aggregate,all_batches_pass:true,coverage_exact_once:true,counts:$counts[0],
      closed_pid:false,closed_cwd:false,closed_fd:false,closed_dso:false,retirement_credit:0}' > "$index/complete.json"
  IFS=' ' read -r now _ < /proc/uptime
  if ! awk -v start="$started" -v now="$now" 'BEGIN{exit !(now>=start && now-start<=180)}'; then
    rm -- "$index/complete.json"
    fail 'fixture batch deadline exceeded before completion'
  fi
  hold "$index/complete.json"
}
for spec in 'cargo-trace checker checker-build' 'cargo-trace probe probe-build' 'cargo-trace fixture fixture-build' 'reference-trace reference reference-build'; do
  read -r directory prefix label <<< "$spec"
  if [[ $prefix == fixture ]]; then fixture_batches; continue; fi
  run "inspect-$prefix-build" 180 none unused unused build "$checker" inspect-trace --directory "$proof/$directory" --prefix "$prefix" --output "$proof/inspect/$prefix-build.json"
  jq -e --arg ref "$fresh_reference" --arg probe "$probe" --arg prefix "$prefix" \
    --slurpfile argv "$proof/build/$label.argv.expected.json" --slurpfile env "$proof/build/$label.env.expected.json" \
    --slurpfile cpp_command "$proof/reference-command.expected.json" '
    [.events[]|select(.exec!=null)] as $execs |
    [$execs[]|select(.exec.program.bytes==($argv[0][0]|explode))] as $roots |
    .raw_immutable==true and .full_acceptance==false and .retirement_credit==false and
    .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and
    ($roots|length)==1 and $roots[0].operation=="execve" and $roots[0].outcome=="exec_succeeded" and
    ($roots[0].exec.argv|map(.bytes))==($argv[0]|map(explode)) and
    $roots[0].exec.environment.complete==true and
    ($roots[0].exec.environment.entries|map(.bytes)|sort)==($env[0]|map(explode)|sort) and
    all($execs[]; .exec.program.bytes!=($ref|explode) and .exec.program.bytes!=($probe|explode)) and
    (if $prefix=="reference" then
      ([$execs[]|select(.outcome=="exec_succeeded" and (.exec.argv|map(.bytes))==($cpp_command[0]|map(explode)) and
        .exec.environment.complete==true and (.exec.environment.entries|map(.bytes)|sort)==($env[0]|map(explode)|sort))]|length)==1
      else true end)
    ' "$proof/inspect/$prefix-build.json" > "$proof/inspect/$prefix-build.check"
done
for prefix in checker probe reference; do audit_build_programs "$proof/inspect/$prefix-build.json" "$prefix"; done
verify_held
phase=collision8-observed
for path in "$proof/inspect/collision.json" "$proof/case-manifest.jsonl" "$proof/current-artifacts.json"; do hold "$path"; done
jq -n --arg source_policy "$source_policy" --arg source "$GITHUB_SHA" --arg checker "$checker" --arg fixture "$fixture" \
  --arg lexical_sha "${held_sha[$proof/inspect/collision.json]}" \
  --arg cases_sha "${held_sha[$proof/case-manifest.jsonl]}" \
  --arg artifacts_sha "${held_sha[$proof/current-artifacts.json]}" \
  --arg batches_sha "${held_sha[$proof/fixture-batches/complete.json]}" \
  --slurpfile cases "$proof/case-manifest.jsonl" \
  '{schema:1,source_policy:$source_policy,source_commit:$source,selector:"actual_cli_foreign_output_collisions_precede_builder_or_provider_work",
    selector_count:1,logical_cases:8,case_keys:[$cases[]|[.prebuilt,.kind]],checker:$checker,fixture:$fixture,
    checker_execs:8,checker_exit1:8,checker_spawn_attempts:0,harness_pre_exec_session_setup:"none in pinned fixture",
    original_cpp_executions:0,probe_executions:0,
    foreign_before_after_equal:true,lexical_report:"inspect/collision.json",lexical_sha256:$lexical_sha,
    case_manifest_sha256:$cases_sha,current_artifacts_sha256:$artifacts_sha,fixture_batches_sha256:$batches_sha,historical_pass_inherited:false,
    old27_admitted:false,runtime2_admitted:false,caller_admitted:false,source_absence_admitted:false,
    closed_pid:false,closed_cwd:false,closed_fd:false,closed_dso:false,retirement_credit:0}' > "$proof/collision8.candidate.json"
hold "$proof/collision8.candidate.json"
passed=true
