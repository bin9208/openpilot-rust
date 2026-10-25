#!/usr/bin/env bash
# #523: SSH entry4와 삭제1의 sampled absence 후보. 원본 실행/retirement credit은 없다.
set -eEuo pipefail
set +m
umask 077
[[ $# == 0 && ${GITHUB_ACTIONS:-} == true && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust ]]
[[ ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:X64 && $(uname -m) == x86_64 ]]
[[ ${GITHUB_SHA:-} =~ ^[a-f0-9]{40}$ ]]
source=${DIAG_SOURCE:?}; job=${CI_JOB_TEMP:?}
[[ $PWD == "$source" && $source == /* && $job == /* && $source != "$job" ]]
for path in "$source" "$job" "$job/proof"; do
  [[ $path =~ ^/[A-Za-z0-9_./+-]+$ && ! -L $path && $(readlink -e "$path") == "$path" && $(stat -c %u "$path") == "$UID" ]]
done
[[ $(stat -c %a "$job") == 700 && $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" ]]
[[ $(git --no-optional-locks -C "$source" rev-parse HEAD) == "$GITHUB_SHA" ]]
sdk=$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu
[[ ${CI_PREPARED_RUST_ROOT:-} == "$sdk" && ${CI_PREPARED_RUSTUP_ROOT:-} == "$job/rustup-home" &&
   ${CI_PREPARED_RUSTUP_BIN:-} == "$job/cargo-home/bin/rustup" &&
   ${CI_PREPARED_RUST_MANIFEST:-} == "$job/proof/prepared-rust-files.before.sha256" && ${CARGO_HOME:-} == "$job/cargo-home" ]]
for path in "$job/rustup-home" "$sdk" "$job/cargo-home" "$job/cargo-home/bin"; do
  [[ -d $path && ! -L $path && $(readlink -e "$path") == "$path" && $(stat -c %u "$path") == "$UID" ]]
done
proof=$job/proof/developer-ssh-entry
mkdir -m 700 "$proof" "$proof/driver"
driver=$proof/driver
build=$job/build
if [[ ! -e $build && ! -L $build ]]; then mkdir -m 700 "$build"; fi
[[ ! -L $build && $(readlink -e "$build") == "$build" && $(stat -c '%u:%a' "$build") == "$UID:700" ]]
mkdir -m 700 "$build/cargo-home" "$build/target" "$build/ssh-entry-tmp" "$driver/trace"
phase=admission
passed=false registry_sealed=false
absence_ready=false
active= active_start= watcher= stop=
declare -A held_fd=() held_id=() held_sha=() aliases=() alias_id=() elf_seen=()
declare -a registry_roots=() registry_names=()
idfile() { stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$1"; }
fail() { printf 'SSH entry: %s\n' "$*" >&2; exit 1; }
absence() {
  [[ $absence_ready == true ]] || return 43
  ssh_absence_verify "$source" "$GITHUB_SHA" "$1"
}
hold() {
  local path=$1 canonical before fd digest
  [[ $path =~ ^/[A-Za-z0-9_./+-]+$ ]] || return 1
  canonical=$(readlink -e -- "$path") || return 1
  [[ -f $canonical && ! -L $canonical && $(stat -c %s "$canonical") -le 536870912 ]] || return 1
  [[ $(stat -c %u "$canonical") == 0 || $(stat -c %u "$canonical") == "$UID" ]] || return 1
  before=$(idfile "$canonical") || return 1
  if [[ -v aliases[$path] ]]; then
    [[ ${aliases[$path]} == "$canonical" && ${alias_id[$path]} == "$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path")" ]] || return 1
  else
    aliases[$path]=$canonical
    alias_id[$path]=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") || return 1
  fi
  if [[ -v held_fd[$canonical] ]]; then [[ ${held_id[$canonical]} == "$before" ]]; return; fi
  exec {fd}<"$canonical" || return 1
  [[ $(idfile "/proc/$$/fd/$fd") == "$before" ]] || return 1
  digest=$(sha256sum "/proc/$$/fd/$fd") || return 1; digest=${digest%% *}
  [[ $(idfile "$canonical") == "$before" && $(idfile "/proc/$$/fd/$fd") == "$before" ]] || return 1
  held_fd[$canonical]=$fd; held_id[$canonical]=$before; held_sha[$canonical]=$digest
  printf '%s\t%s\t%s\n' "$canonical" "$before" "$digest" >> "$driver/held.before.tsv"
}
verify_held() {
  local path fd hash result=0
  for path in "${!aliases[@]}"; do
    [[ $(readlink -e "$path") == "${aliases[$path]}" && $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") == "${alias_id[$path]}" ]] || result=1
  done
  for path in "${!held_fd[@]}"; do
    fd=${held_fd[$path]}
    [[ $(idfile "$path") == "${held_id[$path]}" && $(idfile "/proc/$$/fd/$fd") == "${held_id[$path]}" ]] || { result=1; continue; }
    hash=$(sha256sum "/proc/$$/fd/$fd") || { result=1; continue; }; hash=${hash%% *}
    [[ $hash == "${held_sha[$path]}" && $(idfile "$path") == "${held_id[$path]}" ]] || result=1
    printf '%s\t%s\t%s\n' "$path" "${held_id[$path]}" "$hash" >> "$driver/held.after.tsv" || result=1
  done
  return "$result"
}
# 원문 ELF/NEEDED만 읽는다. SDK를 다시 설치하거나 ldd로 후보를 실행하지 않는다.
bind_elf() {
  local canonical needed interp candidate root resolved path
  hold "$1" || return 1
  canonical=$(readlink -e "$1") || return 1
  [[ -v elf_seen[$canonical] ]] && return 0
  elf_seen[$canonical]=true
  readelf -h "$canonical" > "$driver/elf.header" || return 1
  grep -Eq 'Class: +ELF64$' "$driver/elf.header" || return 1
  grep -Eq 'Machine: +Advanced Micro Devices X86-64$' "$driver/elf.header" || return 1
  grep -Eq 'Type: +(DYN|EXEC) ' "$driver/elf.header" || return 1
  readelf -l "$canonical" > "$driver/elf.program" || return 1
  readelf -d "$canonical" > "$driver/elf.dynamic" || return 1
  printf '%s\n' "$canonical" >> "$driver/elfs.txt"
  cat "$driver/elf.header" "$driver/elf.program" "$driver/elf.dynamic" >> "$driver/elf-details.txt" || return 1
  interp=$(sed -n 's/.*Requesting program interpreter: \([^]]*\)\].*/\1/p' "$driver/elf.program") || return 1
  if [[ -n $interp ]]; then [[ $interp == /lib64/ld-linux-x86-64.so.2 ]] || return 1; hold "$interp" || return 1; fi
  needed=$(sed -n 's/.*(NEEDED).*Shared library: \[\([^]]*\)\].*/\1/p' "$driver/elf.dynamic") || return 1
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
    [[ -n $resolved ]] && bind_elf "$resolved" || return 1
  done <<< "$needed"
}
tree() {
  local root=$1 out=$2
  find -P "$root" -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$out.roster" || return 1
  find -P "$root" -type f -print0 | sort -z | xargs -0 -r sha256sum > "$out.sha256" || return 1
  find -P "$root" -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$out.stable" || return 1
  cmp "$out.roster" "$out.stable"
}
usage_identity() {
  [[ -d $1 && ! -L $1 && $(readlink -e -- "$1") == "$1" && $(stat -c %u -- "$1") == "$UID" ]] || return 1
  # mutable directory의 mtime/nlink는 고정하지 않는다.
  stat -c '%d:%i:%f:%u:%g' -- "$1"
}
# 0=깨끗한 전체 du, 75=target ENOENT 재관측, 1=즉시 실패. 원문을 고치지 않는다.
usage_result() {
  local usage_root=$1 usage_before=$2 usage_target_before=$3 usage_cap=$4 usage_code=$5 usage_out=$6 usage_err=$7
  local usage_value usage_bytes_value usage_line usage_relative usage_raw
  local LC_ALL=C
  [[ $(usage_identity "$usage_root") == "$usage_before" ]] || { printf 'root-identity\n'; return 1; }
  if [[ $usage_root == "$build" || $usage_root == "$build/target" ]]; then
    [[ $(usage_identity "$build/target") == "$usage_target_before" ]] || { printf 'target-identity\n'; return 1; }
  fi
  for usage_raw in "$usage_out" "$usage_err"; do
    [[ -f $usage_raw && ! -L $usage_raw && $(stat -c '%u:%h' -- "$usage_raw") == "$UID:1" &&
       $(stat -c %s -- "$usage_raw") -lt 65536 ]] || { printf 'raw-bound\n'; return 1; }
  done
  usage_value=$(cat -- "$usage_out") || return 1
  # 정확한 단일 LF 행. command substitution의 NUL/LF 소실도 byte 대조로 거부한다.
  printf '%s\n' "$usage_value" | cmp -s - "$usage_out" || { printf 'output-shape\n'; return 1; }
  usage_bytes_value=${usage_value%%$'\t'*}
  [[ $usage_bytes_value =~ ^(0|[1-9][0-9]*)$ && $usage_value == "$usage_bytes_value"$'\t'"$usage_root" ]] || { printf 'output-shape\n'; return 1; }
  # 산술 변환 전에 비교하여 overflow와 failed scan의 부분 합 초과도 즉시 거부한다.
  if (( ${#usage_bytes_value} > ${#usage_cap} )) ||
     { (( ${#usage_bytes_value} == ${#usage_cap} )) && [[ $usage_bytes_value > $usage_cap ]]; }; then
    printf 'partial-cap\n'; return 1
  fi
  if [[ $usage_code == 0 && ! -s $usage_err ]]; then printf 'clean:%s\n' "$usage_bytes_value"; return 0; fi
  [[ ( $usage_root == "$build" || $usage_root == "$build/target" ) && $usage_code == 1 && -s $usage_err ]] || { printf 'status\n'; return 1; }
  while IFS= read -r usage_line || [[ -n $usage_line ]]; do
    [[ $usage_line == "du: cannot access '$build/target/"*"': No such file or directory" ]] || { printf 'error-line\n'; return 1; }
    usage_relative=${usage_line#"du: cannot access '$build/target/"}
    usage_relative=${usage_relative%"': No such file or directory"}
    [[ $usage_relative =~ ^[A-Za-z0-9_./+-]+$ ]] || { printf 'error-path\n'; return 1; }
    case "/$usage_relative/" in *'/../'*|*'/./'*|*'//'*) printf 'error-components\n'; return 1;; esac
    [[ ! -e $build/target/$usage_relative && ! -L $build/target/$usage_relative ]] || { printf 'error-present\n'; return 1; }
  done < "$usage_err"
  # read는 NUL을 버릴 수 있다. 모든 행을 동일하게 재구성할 수 있어야 한다.
  usage_value=$(cat -- "$usage_err") || return 1
  printf '%s\n' "$usage_value" | cmp -s - "$usage_err" || { printf 'error-shape\n'; return 1; }
  printf 'target-enoent\n'; return 75
}
usage_failure() {
  # command substitution 안의 실패도 파일로 sticky하게 남긴다.
  (set -C; printf '%s\n' "$phase" > "$driver/resource-failure") 2>/dev/null || true
  return 1
}
usage_bytes() {
  local usage_root=$1 usage_cap=$2 usage_admission=$3 usage_before usage_target_before=unused
  local usage_tag usage_dir usage_attempt usage_code usage_decision usage_reason usage_classified_reason usage_after usage_target_after
  case "$usage_root" in
    "$build") usage_tag=build ;;
    "$build/target") usage_tag=target ;;
    "$build/cargo-home") usage_tag=cargo ;;
    "$proof") usage_tag=proof ;;
    "$job/cargo-home/registry") usage_tag=registry ;;
    "$job/cargo-home/git") usage_tag=git ;;
    *) usage_failure; return 1 ;;
  esac
  [[ $usage_cap =~ ^[1-9][0-9]{0,9}$ ]] || { usage_failure; return 1; }
  usage_before=$(usage_identity "$usage_root") || { usage_failure; return 1; }
  if [[ $usage_root == "$build" || $usage_root == "$build/target" ]]; then
    usage_target_before=$(usage_identity "$build/target") || { usage_failure; return 1; }
  fi
  usage_dir=$driver/usage.$usage_tag.$BASHPID.$EPOCHREALTIME
  mkdir -m 700 -- "$usage_dir" || { usage_failure; return 1; }
  for usage_attempt in 1 2 3; do
    usage_code=0; usage_decision=0
    [[ $(usage_identity "$usage_root") == "$usage_before" ]] || { usage_failure; return 1; }
    # 파일 상한은 이 du subprocess의 원문 출력에만 적용하며 Cargo/ELF에는 전파하지 않는다.
    (trap - EXIT ERR INT TERM HUP; ulimit -c 0 && ulimit -f 64 && LC_ALL=C du -sx --block-size=1 -- "$usage_root") \
      > "$usage_dir/$usage_attempt.stdout" 2> "$usage_dir/$usage_attempt.stderr" || usage_code=$?
    usage_reason=$(usage_result "$usage_root" "$usage_before" "$usage_target_before" "$usage_cap" "$usage_code" \
      "$usage_dir/$usage_attempt.stdout" "$usage_dir/$usage_attempt.stderr") || usage_decision=$?
    usage_classified_reason=$usage_reason
    usage_after=$(usage_identity "$usage_root") || usage_after=unavailable
    usage_target_after=unused
    if [[ $usage_root == "$build" || $usage_root == "$build/target" ]]; then
      usage_target_after=$(usage_identity "$build/target") || usage_target_after=unavailable
    fi
    if [[ $usage_after != "$usage_before" ]]; then usage_decision=1; usage_reason=root-identity-after
    elif [[ $usage_target_after != "$usage_target_before" ]]; then usage_decision=1; usage_reason=target-identity-after
    fi
    (set -C; printf 'phase=%s\nadmission=%s\nroot=%s\nattempt=%s\nroot_before=%s\nroot_after=%s\ntarget_before=%s\ntarget_after=%s\ncap=%s\ndu_code=%s\ndecision=%s\nclassified_reason=%s\nreason=%s\n' \
      "$phase" "$usage_admission" "$usage_root" "$usage_attempt" "$usage_before" "$usage_after" \
      "$usage_target_before" "$usage_target_after" "$usage_cap" "$usage_code" "$usage_decision" "$usage_classified_reason" "$usage_reason" > "$usage_dir/$usage_attempt.meta") || { usage_failure; return 1; }
    if [[ $usage_decision == 0 ]]; then printf '%s\n' "${usage_reason#clean:}"; return 0; fi
    if [[ $usage_attempt == 1 ]]; then
      (set -C; printf '%s\n' "$usage_dir/$usage_attempt.meta" > "$usage_dir/first-failure") || { usage_failure; return 1; }
    fi
    [[ $usage_decision == 75 ]] || { usage_failure; return 1; }
  done
  (set -C; printf 'attempts=3\nreason=retry-exhausted\n' > "$usage_dir/exhausted") || true
  usage_failure
}

# GHA에서만 호출하는 authored preflight. 실제 du/compile/SSH 수용을 대신하지 않는다.
usage_controls() {
  local usage_controls=$driver/usage-controls usage_control_identity usage_control_reason usage_control_code usage_control_case usage_control_expected
  local usage_control_dir usage_control_decision usage_control_expected_reason
  local usage_control_root=$build/target usage_control_fixture=$build/target/ssh-usage-controls
  local -a usage_control_scans=()
  local phase=usage-controls
  mkdir -m 700 "$usage_controls" "$usage_control_fixture"
  : > "$usage_control_fixture/present"
  ln -s absent "$usage_control_fixture/dangling"
  usage_control_identity=$(usage_identity "$usage_control_root")
  for usage_control_case in clean target-enoent build-target-enoent partial-cap outside permission present dangling traversal status overflow identity malformed nul-stdout nul-stderr mixed raw-bound; do
    usage_control_dir=$usage_controls/$usage_control_case
    mkdir -m 700 "$usage_control_dir"
    usage_control_root=$build/target
    [[ $usage_control_case != build-target-enoent ]] || usage_control_root=$build
    usage_control_identity=$(usage_identity "$usage_control_root")
    printf '9\t%s\n' "$usage_control_root" > "$usage_control_dir/stdout"
    usage_control_code=1; usage_control_expected=1
    printf "du: cannot access '%s/target/ssh-usage-controls/missing': No such file or directory\n" "$build" > "$usage_control_dir/stderr"
    case "$usage_control_case" in
      clean) usage_control_code=0; usage_control_expected=0; usage_control_expected_reason=clean:9; : > "$usage_control_dir/stderr" ;;
      target-enoent|build-target-enoent) usage_control_expected=75; usage_control_expected_reason=target-enoent ;;
      partial-cap) usage_control_expected_reason=partial-cap; printf '11\t%s\n' "$usage_control_root" > "$usage_control_dir/stdout" ;;
      outside) usage_control_expected_reason=error-line; printf "du: cannot access '%s/cargo-home/missing': No such file or directory\n" "$build" > "$usage_control_dir/stderr" ;;
      permission) usage_control_expected_reason=error-line; printf "du: cannot read directory '%s/private': Permission denied\n" "$usage_control_root" > "$usage_control_dir/stderr" ;;
      present|dangling|traversal)
        usage_control_expected_reason=error-present
        usage_control_reason=ssh-usage-controls/$usage_control_case
        [[ $usage_control_case != traversal ]] || { usage_control_reason=../missing; usage_control_expected_reason=error-components; }
        printf "du: cannot access '%s/%s': No such file or directory\n" "$usage_control_root" "$usage_control_reason" > "$usage_control_dir/stderr" ;;
      status) usage_control_expected_reason=status; usage_control_code=2 ;;
      overflow) usage_control_expected_reason=partial-cap; printf '18446744073709551616\t%s\n' "$usage_control_root" > "$usage_control_dir/stdout" ;;
      identity) usage_control_expected_reason=root-identity; usage_control_identity=wrong-identity ;;
      malformed) usage_control_expected_reason=output-shape; printf '9\t%s\nextra\n' "$usage_control_root" > "$usage_control_dir/stdout" ;;
      nul-stdout) usage_control_expected_reason=output-shape; printf '9\t%s\0' "$usage_control_root" > "$usage_control_dir/stdout" ;;
      nul-stderr) usage_control_expected_reason=error-shape; printf "du: cannot access '%s/ssh-usage-controls/missing': No such file or directory\0" "$usage_control_root" > "$usage_control_dir/stderr" ;;
      mixed) usage_control_expected_reason=error-line; printf 'du: Permission denied\n' >> "$usage_control_dir/stderr" ;;
      raw-bound) usage_control_expected_reason=raw-bound; printf '%*s' 65536 '' > "$usage_control_dir/stderr" ;;
    esac
    usage_control_decision=0
    usage_result "$usage_control_root" "$usage_control_identity" "$(usage_identity "$build/target")" \
      10 "$usage_control_code" "$usage_control_dir/stdout" "$usage_control_dir/stderr" \
      > "$usage_control_dir/reason" 2> "$usage_control_dir/parse.stderr" || usage_control_decision=$?
    [[ $usage_control_decision == "$usage_control_expected" && $(cat "$usage_control_dir/reason") == "$usage_control_expected_reason" ]]
    printf '%s\t%s\t%s\n' "$usage_control_case" "$usage_control_decision" "$usage_control_expected_reason" >> "$driver/usage-controls.tsv"
  done
  for usage_control_case in then-clean exhausted cap-first; do
    mkdir -m 700 "$usage_controls/$usage_control_case"
    usage_control_code=0
    (
      # 함수 mock은 이 합성 subshell에만 존재한다. PATH/provider override는 없다.
      trap - EXIT ERR INT TERM HUP
      driver=$usage_controls/$usage_control_case
      printf '0\n' > "$driver/calls"
      du() {
        local usage_mock_count
        [[ $# == 4 && $1 == -sx && $2 == --block-size=1 && $3 == -- && $4 == "$build/target" ]] || return 99
        IFS= read -r usage_mock_count < "$driver/calls" || return 99
        [[ $usage_mock_count =~ ^[0-2]$ ]] || return 99
        usage_mock_count=$((usage_mock_count+1)); printf '%s\n' "$usage_mock_count" > "$driver/calls"
        if [[ $usage_control_case == then-clean && $usage_mock_count == 3 ]]; then printf '9\t%s\n' "$build/target"; return 0; fi
        if [[ $usage_control_case == cap-first ]]; then printf '11\t%s\n' "$build/target"; else printf '9\t%s\n' "$build/target"; fi
        printf "du: cannot access '%s/ssh-usage-controls/missing': No such file or directory\n" "$build/target" >&2
        return 1
      }
      usage_bytes "$build/target" 10 selftest > "$driver/result"
    ) || usage_control_code=$?
    if [[ $usage_control_case == then-clean ]]; then
      [[ $usage_control_code == 0 && $(cat "$usage_controls/$usage_control_case/result") == 9 && ! -e $usage_controls/$usage_control_case/resource-failure ]]
    else
      [[ $usage_control_code == 1 && -f $usage_controls/$usage_control_case/resource-failure && ! -s $usage_controls/$usage_control_case/result ]]
    fi
    usage_control_expected=3; [[ $usage_control_case != cap-first ]] || usage_control_expected=1
    [[ $(cat "$usage_controls/$usage_control_case/calls") == "$usage_control_expected" ]]
    usage_control_scans=("$usage_controls/$usage_control_case"/usage.target.*)
    [[ ${#usage_control_scans[@]} == 1 && -d ${usage_control_scans[0]} && -f ${usage_control_scans[0]}/first-failure ]]
    case "$usage_control_case" in
      then-clean)
        grep -Fxq 'du_code=0' "${usage_control_scans[0]}/3.meta"
        grep -Fxq 'reason=clean:9' "${usage_control_scans[0]}/3.meta" ;;
      exhausted)
        [[ $(cat "${usage_control_scans[0]}/exhausted") == $'attempts=3\nreason=retry-exhausted' ]]
        grep -Fxq 'reason=target-enoent' "${usage_control_scans[0]}/3.meta" ;;
      cap-first)
        [[ ! -e ${usage_control_scans[0]}/2.meta ]]
        grep -Fxq 'reason=partial-cap' "${usage_control_scans[0]}/1.meta" ;;
    esac
    printf '%s\t%s\tattempts=%s\n' "$usage_control_case" "$usage_control_code" "$usage_control_expected" >> "$driver/usage-controls.tsv"
  done
  [[ $(wc -l < "$driver/usage-controls.tsv") == 20 ]]
  printf 'SSH_USAGE_PREFLIGHT cases=20 passed=true synthetic_only=true\n' > "$driver/usage-controls.pass"
}
# 6GiB target + 2GiB Cargo home + 512MiB proof. reserve는 예상 growth와 별도다.
admit() {
  local label=$1 target_growth=$2 cargo_growth=$3 proof_growth=$4 path dev available need target_bytes cargo_bytes proof_bytes build_bytes absence_bytes
  target_bytes=$(usage_bytes "$build/target" $((6144*1048576)) "$label") || return 1
  cargo_bytes=$(usage_bytes "$build/cargo-home" $((2048*1048576)) "$label") || return 1
  proof_bytes=$(usage_bytes "$proof" $((512*1048576)) "$label") || return 1
  # 공통 proof에 둔 작은 absence journal/sidecar도 기존512MiB에 합산한다.
  absence_bytes=$(du -scx --block-size=1 -- "$job/proof"/ssh-absence-* \
    "$job/proof/ssh-original-deletion.diff" "$job/proof/ssh-historical-original.sha256") || return 1
  absence_bytes=${absence_bytes##*$'\n'}; absence_bytes=${absence_bytes%%$'\t'*}
  [[ $absence_bytes =~ ^[0-9]+$ ]] || return 1
  proof_bytes=$((proof_bytes+absence_bytes))
  build_bytes=$(usage_bytes "$build" $((8192*1048576)) "$label") || return 1
  (( target_bytes <= 6144*1048576 && cargo_bytes <= 2048*1048576 && build_bytes <= 8192*1048576 && proof_bytes <= 512*1048576 )) || return 1
  # 각 실제 mount의 수요를 합산하며 같은 filesystem의 요구를 중복해서 빼지 않는다.
  local -A demand=() representative=()
  for path in "$build/target" "$build/cargo-home" "$proof"; do
    dev=$(stat -c %d "$path") || return 1
    representative[$dev]=$path
    [[ -v demand[$dev] ]] || demand[$dev]=0
    case "$path" in
      "$build/target") need=$target_growth ;;
      "$build/cargo-home") need=$cargo_growth ;;
      *) need=$proof_growth ;;
    esac
    demand[$dev]=$((demand[$dev]+need*1048576))
  done
  for dev in "${!demand[@]}"; do
    available=$(/usr/bin/df --output=avail -B1 -- "${representative[$dev]}" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {n++;v=$1} END {if(NR!=2 || n!=1) exit 1;print v}') || return 1
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$dev" "$available" "${demand[$dev]}" "$target_bytes" "$cargo_bytes" "$proof_bytes" >> "$driver/growth.tsv" || return 1
    # 종료 증거512MiB + inflight 여유1GiB. 여러 PID trace의 write peak 보장은 아니다.
    (( available >= demand[$dev]+1536*1048576 )) || return 1
  done
}
pid_start() {
  local line rest; local -a fields
  IFS= read -r line < "/proc/$1/stat" || return 1
  [[ ${line%% *} == "$1" ]] || return 1
  rest=${line##*) }; read -r -a fields <<< "$rest"
  [[ ${#fields[@]} -ge 20 && ${fields[0]} != Z && ${fields[1]} == "$$" && ${fields[2]} == "$1" && ${fields[3]} == "$1" ]] || return 1
  printf '%s\n' "${fields[19]}"
}
kill_owned() {
  [[ -n $active && -n $active_start && $(pid_start "$active") == "$active_start" ]] || return 1
  kill -KILL -- "-$active"
}
group_empty() {
  local path line rest; local -a fields
  for path in /proc/[0-9]*/stat; do
    if ! IFS= read -r line < "$path"; then [[ ! -e $path ]] && continue; return 1; fi
    rest=${line##*) }; read -r -a fields <<< "$rest"
    [[ ${#fields[@]} -ge 20 && ${fields[2]} =~ ^[0-9]+$ && ${fields[2]} != "$1" ]] || return 1
  done
  printf '%s\tabsent-after-direct-wait\n' "$1" >> "$driver/groups.tsv"
}
finish() {
  local original=$? status=0 n source_absence=false
  trap - EXIT INT TERM HUP
  set +e
  if [[ -n $active ]]; then
    kill_owned || status=1
    wait "$active"
    group_empty "$active" || status=1
  fi
  if [[ -n $watcher ]]; then : > "$stop"; wait "$watcher" || status=1; fi
  absence driver-before-seals || status=1
  verify_held > "$driver/held-final.stdout" 2> "$driver/held-final.stderr" || status=1
  if [[ $registry_sealed == true ]]; then
    for n in "${!registry_roots[@]}"; do
      tree "${registry_roots[$n]}" "$driver/registry-${registry_names[$n]}.after" &&
        cmp "$driver/registry-${registry_names[$n]}.before.roster" "$driver/registry-${registry_names[$n]}.after.roster" &&
        cmp "$driver/registry-${registry_names[$n]}.before.sha256" "$driver/registry-${registry_names[$n]}.after.sha256" || status=1
      if [[ -f $driver/copied-${registry_names[$n]}.before.roster ]]; then
        tree "$build/cargo-home/${registry_names[$n]}" "$driver/copied-${registry_names[$n]}.after" &&
          cmp "$driver/copied-${registry_names[$n]}.before.roster" "$driver/copied-${registry_names[$n]}.after.roster" &&
          cmp "$driver/copied-${registry_names[$n]}.before.sha256" "$driver/copied-${registry_names[$n]}.after.sha256" || status=1
      else status=1
      fi
    done
  else status=1
  fi
  admit final 0 0 0 || status=1
  [[ ! -e $driver/resource-failure ]] || status=1
  absence driver-before-receipt || status=1
  absence driver-before-publish || status=1
  (( status != 0 )) || source_absence=true
  jq -n --arg source "$GITHUB_SHA" --arg phase "$phase" --argjson command "$original" --argjson final "$status" --argjson pass "$passed" --argjson absence "$source_absence" '
    {schema:1,source_sha:$source,phase:$phase,command_exit:$command,finalizer_exit:$final,
     entry4_candidate:($command==0 and $final==0 and $pass),whole_source_sdk_seal:"parent always-final required",
     original_executed:false,source_phase:"absent",removed_source:"tools/scripts/ssh.py",
     historical_source_sha256:"f4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff",
     original_deleted:$absence,source_absence_admitted:$absence,retirement_credit:0,
     absence_scope:"driver command boundaries and 2s sampling; parent final closure required",whole_python_absence:false,
     real_ssh:false,closed_trace:false,complete_build_dependency_closure:false,
     trace_scope:"raw runtime exec/mmap; held provider and executable file mapping checks, not full syscall decoding",
     recovery_scope:"direct owned leader wait/group scan; fixture has independent subreaper receipts",
     resource_scope:"target6GiB/cargo-home2GiB/build8GiB/proof512MiB, sampled growth and 512MiB child fsize",
     measured_peak_guarantee:false}' > "$driver/outcome.json" || status=1
  (( original == 0 && status == 0 )) && [[ $passed == true ]] || exit 1
}
trap finish EXIT
trap 'printf "SSH entry failure phase=%s line=%s exit=%s\n" "$phase" "$LINENO" "$?" >&2' ERR
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
run() {
  local label=$1 seconds=$2 code=0 watched=0 checked=0 leader ready go count
  shift 2
  phase=$label
  absence "command-before-$label"
  admit "$label-before" 0 0 0
  [[ ! -e $driver/resource-failure ]]
  local -a payload=(/usr/bin/timeout --foreground --signal=TERM --kill-after=15s "${seconds}s")
  payload+=("$@")
  printf '%s\0' "$label" "$@" >> "$driver/commands.nul"
  ready=$driver/$label.ready; go=$driver/$label.go; stop=$driver/$label.stop
  /usr/bin/env -i HOME="$job/home" PATH=/usr/bin:/bin LC_ALL=C /usr/bin/setsid /usr/bin/bash --noprofile --norc -c '
    set -euo pipefail
    ulimit -c 0; ulimit -f 524288
    ready=$1; go=$2; shift 2
    (set -o noclobber; printf "%s\n" "$$" > "$ready")
    for ((i=0;i<600;i++)); do [[ ! -e $go ]] || break; /usr/bin/sleep 0.05; done
    [[ -e $go ]]
    exec "$@"
  ' ssh-entry-owned "$ready" "$go" "${payload[@]}" > "$driver/$label.stdout" 2> "$driver/$label.stderr" < /dev/null &
  leader=$!; active=$leader; active_start=
  for ((count=0;count<600;count++)); do
    if [[ -f $ready && $(cat "$ready") == "$leader" ]]; then active_start=$(pid_start "$leader") && break; fi
    sleep 0.05
  done
  [[ -n $active_start ]] || fail 'owned leader handshake failed'
  : > "$go"
  (
    while [[ ! -e $stop ]] && [[ $(pid_start "$leader") == "$active_start" ]]; do
      if ! absence sampled-budget || ! admit "$label-watch" 0 0 0; then
        printf '%s\n' "$label" > "$driver/resource-failure"
        kill_owned || true
        exit 1
      fi
      sleep 2
    done
  ) &
  watcher=$!
  wait "$leader" || code=$?
  : > "$stop"
  wait "$watcher" || watched=$?
  active= active_start= watcher=
  # 정상 wait 이후 과거 PGID를 kill하지 않는다.
  group_empty "$leader" || checked=1
  printf '%s\t%s\t%s\n' "$label" "$code" "$watched" >> "$driver/status.tsv"
  absence "command-after-$label" || checked=1
  admit "$label-after" 0 0 0 || checked=1
  (( code == 0 && watched == 0 && checked == 0 )) && [[ ! -e $driver/resource-failure ]]
}
artifact() {
  local label=$1 name=$2 kind=$3 src=$4 tested=$5
  [[ $(stat -c %s "$driver/$label.stdout") -le 67108864 ]]
  jq -ser --arg package "path+file://$source/rust/crates/dev-tools#openpilot-dev-tools@0.1.0" \
    --arg manifest "$source/rust/crates/dev-tools/Cargo.toml" --arg name "$name" --arg kind "$kind" --arg src "$src" --argjson tested "$tested" '
    if length<1 or .[-1].reason!="build-finished" or .[-1].success!=true or
      ([.[]|select(.reason=="build-finished")]|length)!=1 or
      (all(.[];type=="object" and (.reason=="compiler-artifact" or .reason=="compiler-message" or
         .reason=="build-script-executed" or .reason=="build-finished"))|not)
    then error("Cargo JSONL/EOF differs") else . end |
    [.[]|select(.reason=="compiler-artifact" and .package_id==$package and .manifest_path==$manifest and
      .target.name==$name and .target.kind==[$kind] and .target.src_path==$src and .profile.test==$tested and .executable!=null)|.executable] |
    if length==1 and (.[0]|type=="string" and startswith("/")) then .[0] else error("Cargo ELF missing or duplicate") end' "$driver/$label.stdout"
}
bind_json() {
  [[ -f $1 && ! -L $1 && $(stat -c '%u:%h' "$1") == "$UID:1" && $(stat -c %s "$1") -le 8388608 ]] || return 1
  hold "$1"
}

# 공통 absence context는 checkout 때만 생성한다. 여기서 재admit/부재 추측은 하지 않는다.
absence_helper=$source/tools/ci/ssh_source_absence_523.sh
[[ -f $absence_helper && ! -L $absence_helper ]]
hold "$absence_helper"
[[ ${held_sha[$absence_helper]} == 33c8547d622f61758a364c8969ebcadc85b2c2825e4da00b777dddb9e50359fa ]]
. "/proc/$$/fd/${held_fd[$absence_helper]}"
absence_ready=true
absence driver-entry
for path in "$job/proof/ssh-absence-before.tsv" "$job/proof/ssh-absence-head.bin" "$job/proof/ssh-absence-index.bin" \
  "$job/proof/ssh-original-deletion.diff" "$job/proof/ssh-historical-original.sha256"; do hold "$path"; done

# 현재 source/SDK/provider만 hold한다. 역사 SSH SHA는 metadata이며 원본 실행은 없다.
admit provider-baseline 0 0 32
for path in "$source/tools/ci/developer_ssh_entry_523.sh" "$source/rust/Cargo.toml" "$source/rust/Cargo.lock" \
  "$source/rust/crates/dev-tools/Cargo.toml" "$source/rust/crates/dev-tools/tests/developer_ssh_entry.rs" \
  "$source/rust/crates/dev-tools/tests/fixtures/developer-ssh-entry/exec_recorder.rs" "$source/tools/op.sh" "$source/rust/tools/run-developer-tool" \
  "$CI_PREPARED_RUST_MANIFEST" "$job/proof/prepared-rust-programs.sha256" "$job/owner"; do hold "$path"; done
sha256sum --check --strict --quiet "$job/proof/prepared-rust-programs.sha256"
absence sampled-budget
for path in "$sdk/bin/cargo" "$sdk/bin/rustc" "$sdk/bin/rustdoc" "$sdk/bin/clippy-driver" "$CI_PREPARED_RUSTUP_BIN"; do bind_elf "$path"; done
for tool in bash env git df awk basename dirname uname sed find readlink jq strace timeout setsid stat sha256sum du cp readelf sort xargs sleep cat cmp cut grep mkdir wc id ln \
  cc c++ ar as ld ranlib nm pkg-config capnp; do
  path=$(command -v "$tool"); [[ $path == /* ]]; bind_elf "$path"
done
# public shell의 /bin alias도 실행 전 묶는다.
bind_elf /bin/bash
usage_controls
admit registry-before 0 2048 32
registry_bytes=0
for name in registry git; do
  path=$job/cargo-home/$name
  if [[ -e $path || -L $path ]]; then
    [[ -d $path && ! -L $path && $(readlink -e "$path") == "$path" ]]
    rejected=$(find -P "$path" ! -type d ! -type f -printf x)
    [[ -z $rejected ]]
    registry_bytes=$((registry_bytes+$(usage_bytes "$path" $((2016*1048576)) "registry-input-$name")))
    (( registry_bytes <= 2016*1048576 )) # 새 Cargo 상태 파일에32MiB를 남긴다.
    registry_roots+=("$path"); registry_names+=("$name")
    tree "$path" "$driver/registry-$name.before"
  fi
done
(( ${#registry_roots[@]} >= 1 ))
registry_sealed=true
run registry-copy 180 /usr/bin/env -i PATH=/usr/bin:/bin /usr/bin/cp -a --reflink=never -- "${registry_roots[@]}" "$build/cargo-home/"
for name in "${registry_names[@]}"; do
  (cd "$job/cargo-home/$name" && find -P . -type f -print0 | sort -z | xargs -0 -r sha256sum) > "$driver/copy-$name.expected"
  (cd "$build/cargo-home/$name" && find -P . -type f -print0 | sort -z | xargs -0 -r sha256sum) > "$driver/copy-$name.actual"
  cmp "$driver/copy-$name.expected" "$driver/copy-$name.actual"
  tree "$build/cargo-home/$name" "$driver/copied-$name.before"
done
build_env=(HOME="$proof/public/alias/home" PATH="$proof/public/bin:$sdk/bin:$job/cargo-home/bin:/usr/bin:/bin"
  CARGO_HOME="$build/cargo-home" CARGO_TARGET_DIR="$build/target" RUSTUP_HOME="$job/rustup-home" RUSTUP_TOOLCHAIN=1.94.0
  RUSTUP_AUTO_INSTALL=0 RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" CARGO_NET_OFFLINE=true
  CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  GIT_OPTIONAL_LOCKS=0 LANG=C.UTF-8 LC_ALL=C.UTF-8)
common=(--manifest-path "$source/rust/Cargo.toml" -p openpilot-dev-tools --features developer-ssh-native --locked --target x86_64-unknown-linux-gnu)
admit fresh-build 6144 32 128
run public-build 900 /usr/bin/env -i "${build_env[@]}" "$sdk/bin/cargo" build "${common[@]}" --bin developer-ssh --message-format=json
run entry-build 900 /usr/bin/env -i "${build_env[@]}" "$sdk/bin/cargo" test "${common[@]}" --test developer_ssh_entry --no-run --message-format=json
public_cli=$(artifact public-build developer-ssh bin "$source/rust/crates/dev-tools/src/bin/developer_ssh.rs" false)
fixture=$(artifact entry-build developer_ssh_entry test "$source/rust/crates/dev-tools/tests/developer_ssh_entry.rs" true)
cli=$(artifact entry-build developer-ssh bin "$source/rust/crates/dev-tools/src/bin/developer_ssh.rs" false)
[[ $public_cli == "$build/target/x86_64-unknown-linux-gnu/debug/developer-ssh" && $cli == "$public_cli" &&
   $fixture == "$build/target/x86_64-unknown-linux-gnu/debug/deps/"* && $(readlink -e "$fixture") == "$fixture" ]]
recorder=$build/target/developer-ssh-entry-recorder
run recorder-build 120 /usr/bin/env -i "${build_env[@]}" "$sdk/bin/rustc" --edition=2021 -C debuginfo=0 -C opt-level=1 \
  "$source/rust/crates/dev-tools/tests/fixtures/developer-ssh-entry/exec_recorder.rs" -o "$recorder"
bind_elf "$fixture"; bind_elf "$cli"; bind_elf "$recorder"
recorder_sha=${held_sha[$recorder]}
jq -n --arg test "$fixture" --arg cli "$cli" --arg recorder "$recorder" --arg test_sha "${held_sha[$fixture]}" \
  --arg cli_sha "${held_sha[$cli]}" --arg recorder_sha "$recorder_sha" '{test:{path:$test,sha256:$test_sha},cli:{path:$cli,sha256:$cli_sha},recorder:{path:$recorder,sha256:$recorder_sha}}' > "$driver/current-artifacts.json"
runtime=(/usr/bin/env -i PATH=/usr/bin:/bin HOME="$job/home" TMPDIR="$build/ssh-entry-tmp" LANG=C.UTF-8 LC_ALL=C.UTF-8
  GITHUB_ACTIONS=true GITHUB_REPOSITORY=bin9208/openpilot-rust RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux RUNNER_ARCH=X64
  GITHUB_SHA="$GITHUB_SHA" GITHUB_RUN_ID="$GITHUB_RUN_ID" GITHUB_RUN_ATTEMPT="$GITHUB_RUN_ATTEMPT" GITHUB_JOB="$GITHUB_JOB"
  CI296_SOURCE="$source" CI_JOB_TEMP="$job" DEVELOPER_SSH_ENTRY_PROOF="$proof"
  DEVELOPER_SSH_ENTRY_RECORDER="$recorder" DEVELOPER_SSH_ENTRY_RECORDER_SHA256="$recorder_sha"
  CI_PREPARED_RUST_ROOT="$sdk" CI_PREPARED_RUSTUP_ROOT="$job/rustup-home" CI_PREPARED_RUSTUP_BIN="$job/cargo-home/bin/rustup"
  CARGO_HOME="$build/cargo-home" CARGO_TARGET_DIR="$build/target")
run strace-help 15 /usr/bin/env -i PATH=/usr/bin:/bin /usr/bin/strace --help
grep -F -- --kill-on-exit "$driver/strace-help.stdout" > "$driver/exitkill-supported.txt"
# strace 앞에서 env-i를 적용하여 tracer에도 credential/BASH_ENV를 전달하지 않는다.
unset GH_TOKEN GITHUB_TOKEN BASH_ENV
selectors=(entry_held_files_reject_rewrite_replacement_and_new_alias entry_recorder_rejects_bad_nonce_truncation_trailing_and_marker_roster
  actual_native_ssh_entry_boundaries actual_public_op_ssh_entry)
for i in "${!selectors[@]}"; do
  selector=${selectors[$i]}; seconds=30; ignored=()
  # fixture 자체의180/720초는 그대로이며 driver는 회수/receipt에30초를 별도로 둔다.
  if (( i >= 2 )); then ignored=(--ignored); seconds=210; fi
  if (( i == 3 )); then seconds=750; fi
  # runtime의 env-i가 timeout/strace 바깥에 있어 raw env에도 비밀이 없다.
  run "entry-$i" "$seconds" "${runtime[@]}" /usr/bin/strace --kill-on-exit -ff -qq -yy -v -s 65536 \
    -e trace=execve,execveat,mmap -o "$driver/trace/entry-$i" "$fixture" --exact "$selector" "${ignored[@]}" --nocapture --test-threads=1 --color=never
  # actual worker와 바깥 harness의 두 summary는 각각1pass이며 빈 선택을 허용하지 않는다.
  expected=1; (( i < 2 )) || expected=2
  # --nocapture actual marker가 이름과 ok 사이에 출력되므로 한 줄 PASS를 가정하지 않는다.
  [[ $(grep -Fo "test $selector ... " "$driver/entry-$i.stdout" | wc -l) == "$expected" ]]
  if (( i < 2 )); then [[ $(grep -Fxc "test $selector ... ok" "$driver/entry-$i.stdout") == 1 ]]; fi
  [[ $(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$driver/entry-$i.stdout") == "$expected" ]]
  ! grep -Eq '^test result: (FAILED|ok\. 0 passed)' "$driver/entry-$i.stdout"
  [[ ! -s $driver/entry-$i.stderr ]]
done
for mode in native public; do
  calls=7; http=4; execs=2; log=$driver/entry-2.stdout
  case_names=(alias duplicate debug unauthorized help invalid-port missing)
  [[ $mode != public ]] || { calls=1; http=1; execs=1; log=$driver/entry-3.stdout; case_names=(alias); }
  for report in summary.json supervisor.json held.before.json held.after.json per-use-held.json; do bind_json "$proof/$mode/$report"; done
  [[ $(grep -Fxc "DEVELOPER_SSH_ENTRY_COMPLETE mode=$mode calls=$calls http=$http exec=$execs reaped=true" "$log") == 1 ]]
  jq -e --arg source "$GITHUB_SHA" --arg mode "$mode" --arg recorder "$recorder_sha" --argjson calls "$calls" --argjson http "$http" --argjson execs "$execs" '
    .schema_version==1 and .complete==true and .mode==$mode and .source_sha==$source and .recorder_sha256==$recorder and
    .held_verified==true and .reaped==true and .error==null and .real_ssh==false and .installed_provider_closure==false and .retirement_credit==0 and
    (.cases|length)==$calls and ([.cases[].case]|unique|length)==$calls and
    ([.cases[].http]|add)==$http and ([.cases[]|select(.recorder!=null)]|length)==$execs and
    all(.cases[];.token_input_unchanged==true and .reaped==true) and
    (if $mode=="native" then [.cases[].case]==["alias","duplicate","debug","unauthorized","help","invalid-port","missing"] and
      [.cases[].exit]==[0,1,0,1,0,2,2] and [.cases[].http]==[1,1,1,1,0,0,0] and
      [.cases[].token_fifo]==[false,false,false,false,true,true,true]
     else [.cases[].case]==["alias"] and .cases[0].exit==0 and .cases[0].http==1 and .cases[0].token_fifo==false end)' \
    "$proof/$mode/summary.json" > "$driver/$mode-summary.checked"
  jq -e --arg mode "$mode" '.mode==$mode and .worker_code==0 and .worker_error==null and .all_owned_children_reaped==true and .reap_error==null' \
    "$proof/$mode/supervisor.json" > "$driver/$mode-supervisor.checked"
  jq -es 'length==2 and all(.[];type=="array" and length==14 and ([.[].path]|unique|length)==14) and
    (.[0]|sort_by(.path))==(.[1]|sort_by(.path))' "$proof/$mode/held.before.json" "$proof/$mode/held.after.json" > "$driver/$mode-held.checked"
  cmp "$proof/$mode/held.before.json" "$proof/$mode/held.after.json"
  jq -e '.verified_after==true and .error==null and (.before_each_use|type=="array" and length>14)' \
    "$proof/$mode/per-use-held.json" > "$driver/$mode-per-use.checked"
  for case_name in "${case_names[@]}"; do
    bind_json "$proof/$mode/$case_name/case.json"
    jq -es --arg case_name "$case_name" '([.[0].cases[]|select(.case==$case_name)]) as $case |
      ($case|length)==1 and $case[0]==.[1]' "$proof/$mode/summary.json" "$proof/$mode/$case_name/case.json" > "$driver/$mode-$case_name.checked"
  done
done
# exec/mmap에서 읽은 경로만 국소 검사한다. raw syscall 전체 decoder/closed trace라고 주장하지 않는다.
exec_re='^execve\("([^"]+)", .* = 0$'
map_re='[0-9]+<([^>]+)>'
for i in "${!selectors[@]}"; do
  found=0
  for path in "$driver/trace/entry-$i".*; do
    [[ -f $path && ! -L $path && $(stat -c %s "$path") -le 134217728 ]]
    found=$((found+1))
    while IFS= read -r line || [[ -n $line ]]; do
      observed= kind=
      if [[ $line == execveat\(* ]]; then fail 'unexpected execveat'; fi
      if [[ $line =~ $exec_re ]]; then observed=${BASH_REMATCH[1]}; kind=exec
      elif [[ $line == execve\(* ]]; then
        [[ $line == *' = -1 ENOENT '* || $line == *' = -1 ENOTDIR '* || $line == *' = -1 EACCES '* ]] || fail 'opaque or unfinished exec'
      elif [[ $line == *'execve resumed>'* || $line == *'execveat resumed>'* ]]; then fail 'unsupported resumed exec'
      elif [[ $line == mmap\(* && $line == *PROT_EXEC* ]]; then
        if [[ $line =~ $map_re ]]; then observed=${BASH_REMATCH[1]}; kind=executable-map
        else [[ $line == *MAP_ANONYMOUS* && $line == *', -1, '* ]] || fail 'opaque executable mapping'; fi
      fi
      [[ -n $observed ]] || continue
      [[ $observed != rust/tools/run-developer-tool ]] || observed=$source/$observed
      [[ $observed =~ ^/[A-Za-z0-9_./+-]+$ ]] || fail 'unexpected observed path'
      canonical=$(readlink -e "$observed")
      if [[ $canonical == "$proof/native/bin/ssh" || $canonical == "$proof/public/bin/ssh" ]]; then
        [[ $(sha256sum "$canonical" | cut -d' ' -f1) == "$recorder_sha" ]]
        # 사본 실행 전 FD hold는 동결 fixture가 담당한다. driver는 동일 bytes를 후속 검증한다.
        hold "$canonical"
      fi
      [[ -v held_fd[$canonical] && $(idfile "$canonical") == "${held_id[$canonical]}" ]] || fail "unheld runtime $kind: $canonical"
      printf '%s\t%s\t%s\t%s\n' "$i" "$kind" "$canonical" "${held_sha[$canonical]}" >> "$driver/observed.tsv"
    done < "$path"
  done
  (( found >= 1 && found <= 256 ))
  [[ $(awk -F '\t' -v i="$i" -v p="$fixture" '$1==i && $2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == "$((i < 2 ? 1 : 2))" ]]
done
[[ $(awk -F '\t' -v p="$cli" '$1==2 && $2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == 7 ]]
[[ $(awk -F '\t' -v p="$cli" '$1==3 && $2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == 1 ]]
[[ $(awk -F '\t' -v p="$source/rust/tools/run-developer-tool" '$1==3 && $2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == 1 ]]
[[ $(awk -F '\t' -v p="$sdk/bin/cargo" '$1==3 && $2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == 1 ]]
[[ $(awk -F '\t' -v p="$proof/native/bin/ssh" '$2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == 2 ]]
[[ $(awk -F '\t' -v p="$proof/public/bin/ssh" '$2=="exec" && $3==p {n++} END {print n+0}' "$driver/observed.tsv") == 1 ]]
# parser FIFO는 비실행 통제 입력이다. artifact가 지원하지 않는 type도 원문 roster에 남긴다.
find -P "$proof/native" "$proof/public" -printf '%y\t%p\n' | sort > "$driver/case-path-types.tsv"
hold "$driver/case-path-types.tsv"
verify_held
phase=entry4-checked
passed=true
