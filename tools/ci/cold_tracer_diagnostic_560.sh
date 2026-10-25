#!/usr/bin/env bash
# #560 격리 source 계측. 로컬 실행 금지; 원래 Fast/parser/system strace는 수정하지 않는다.
set -euo pipefail
set +m
umask 077
export LC_ALL=C PATH=/usr/bin:/bin
fail() { printf 'cold tracer 560: %s\n' "$*" >&2; exit 1; }
[[ $# == 1 && ( $1 == admit || $1 == run || $1 == run-inherited || $1 == final-source ) ]] || fail 'admit|run|run-inherited|final-source required'
mode=$1
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-} == true:bin9208/openpilot-rust &&
   ${GITHUB_EVENT_NAME:-}:${GITHUB_REF:-} == push:refs/heads/codex/feat-523-python-rust &&
   ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:X64 &&
   $(uname -s):$(uname -m) == Linux:x86_64 ]] || fail 'trusted hosted branch required'
job=${CI_JOB_TEMP:?}; source=${DIAG_SOURCE:?}; owner=${DIAG_SOURCE_OWNER:?}; workspace=${DIAG_ACTIONS_WORKSPACE:?}
[[ $job == /tmp/openpilot-ci.?????? && $source == "$owner/source" && $PWD == "$source" &&
   ${GITHUB_WORKSPACE:-} == "$source" && ${HOME:-} == "$job/home" &&
   ${CARGO_HOME:-} == "$job/cargo-home" && ${RUSTUP_HOME:-} == "$job/rustup-home" ]] || fail 'private layout differs'
case $owner in "$workspace/.cold560-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."??????) ;; *) fail 'source namespace differs';; esac
for path in "$job" "$job/proof" "$source" "$owner"; do
  [[ -d $path && ! -L $path && $(readlink -e "$path") == "$path" && $(stat -c '%u:%a' "$path") == "$(id -u):700" ]] || fail 'owned private root required'
done
[[ $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(cat "$owner/.diag296-owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" &&
   $(stat -c '%d:%i' "$owner") == "$DIAG_SOURCE_IDENTITY" && $GITHUB_SHA =~ ^[0-9a-f]{40}$ &&
   $(git --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || fail 'source/owner binding differs'
printf '%s  %s\n' "${COLD560_HELPER_SHA256:?}" "$source/tools/ci/cold_tracer_diagnostic_560.sh" | sha256sum --check --strict --quiet -
unset GH_TOKEN GITHUB_TOKEN BASH_ENV ENV LD_PRELOAD LD_LIBRARY_PATH RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_TARGET RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1
out=$job/proof/cold560; build=$job/build; cache=$job/openpilot-disk-guard
sdk=$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu
gib=$((1024*1024*1024)); reserve=$((512*1024*1024)); phase=preflight
free_bytes() { df --output=avail -B1 "$1" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {n++;v=$1} END {if(NR!=2||n!=1) exit 1;print v}'; }
admission_failure() {
  # 최초 거부 분기만 보존한다. 종료 admission이 원인을 덮어쓰지 못한다.
  local reason=$1 root=$2 cap=$3 kind=$4 code=$5 bytes=$6 attempt=$7
  local record=$job/proof/cold560-first-admission-failure.tsv
  if [[ ! -e $record && ! -L $record ]];then
    (set -o noclobber;printf 'phase\treason\troot\tcap\tkind\tdu_exit\tobserved_bytes\tattempt\n%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$phase" "$reason" "$root" "$cap" "$kind" "$code" "$bytes" "$attempt" > "$record") || return 1
    if [[ $code =~ ^[0-9]+$ && -f $job/proof/cold560-du.stderr && ! -L $job/proof/cold560-du.stderr &&
          $(stat -c %s "$job/proof/cold560-du.stderr") -le 65536 ]];then
      (set -o noclobber;cat -- "$job/proof/cold560-du.stderr" > "$job/proof/cold560-first-admission-du.stderr") || return 1
    fi
  fi
  return 1
}
measure() {
  local root=$1 cap=$2 apparent=$3 count code value bytes before line path suffix
  [[ -e $root || -L $root ]] || { printf '0\n'; return; }
  [[ -d $root && ! -L $root && $(readlink -e "$root") == "$root" ]] || {
    admission_failure canonical-root "$root" "$cap" "${apparent:-allocated}" - - 0;return 1;
  }
  before=$(stat -c '%d:%i' "$root") || {
    admission_failure root-identity-read "$root" "$cap" "${apparent:-allocated}" - - 0;return 1;
  }
  for count in 1 2 3; do
    code=0
    value=$(du -sx --block-size=1 $apparent -- "$root" 2> "$job/proof/cold560-du.stderr") || code=$?
    [[ $(stat -c '%d:%i' "$root") == "$before" ]] || {
      admission_failure root-identity-changed "$root" "$cap" "${apparent:-allocated}" "$code" - "$count";return 1;
    }
    bytes=${value%%$'\t'*}
    [[ $bytes =~ ^[0-9]+$ && $value == "$bytes"$'\t'"$root" ]] || {
      admission_failure du-output-shape "$root" "$cap" "${apparent:-allocated}" "$code" - "$count";return 1;
    }
    ((bytes<=cap)) || {
      admission_failure measured-cap "$root" "$cap" "${apparent:-allocated}" "$code" "$bytes" "$count";return 1;
    }
    if [[ $code == 0 && ! -s $job/proof/cold560-du.stderr ]]; then printf '%s\n' "$bytes"; return; fi
    [[ $code == 1 && $root == "$build" && -s $job/proof/cold560-du.stderr ]] || {
      admission_failure du-status-or-scope "$root" "$cap" "${apparent:-allocated}" "$code" "$bytes" "$count";return 1;
    }
    # mutable build의 실제 ENOENT만 최대3회 재표본한다. 부분합 cap 초과는 위에서 즉시 실패한다.
    suffix="': No such file or directory"
    while IFS= read -r line; do
      [[ $line == "du: cannot access '$build/"*"$suffix" ]] || {
        admission_failure du-error-shape "$root" "$cap" "${apparent:-allocated}" "$code" "$bytes" "$count";return 1;
      }
      path=${line#"du: cannot access '"};path=${path%"$suffix"}
      [[ $path != *$'\n'* && ! -e $path && ! -L $path ]] || {
        admission_failure du-absent-path "$root" "$cap" "${apparent:-allocated}" "$code" "$bytes" "$count";return 1;
      }
    done < "$job/proof/cold560-du.stderr"
    cat "$job/proof/cold560-du.stderr" >> "$job/proof/cold560-du-races.log" || {
      admission_failure du-race-record "$root" "$cap" "${apparent:-allocated}" "$code" "$bytes" "$count";return 1;
    }
  done
  admission_failure du-three-attempts-exhausted "$root" "$cap" "${apparent:-allocated}" "$code" "$bytes" 3
  return 1
}
admit() {
  local path cap n bytes apparent dev available sdk_bytes=0 sdk_apparent=0
  local -a roots=("$owner" "$build" "$cache" "$job/proof") caps=($((2*gib)) $((8*gib)) "$gib" "$reserve")
  local -A need=() representative=()
  for n in "${!roots[@]}"; do
    path=${roots[$n]};cap=${caps[$n]}
    bytes=$(measure "$path" "$cap" '') || return 1
    apparent=$(measure "$path" "$cap" --apparent-size) || return 1
    dev=$(stat -c %d "$( [[ -d $path ]] && printf '%s' "$path" || printf '%s' "$job" )") || {
      admission_failure filesystem-identity "$path" "$cap" device - - 0;return 1;
    }
    representative[$dev]=$( [[ -d $path ]] && printf '%s' "$path" || printf '%s' "$job" )
    need[$dev]=$(( ${need[$dev]:-0}+cap-bytes ))
  done
  for path in "$job/rustup-home" "$job/cargo-home" "$job/rust-bootstrap"; do
    [[ ! -e $path ]] || [[ $(stat -c %d "$path") == "$(stat -c %d "$job")" ]] || {
      admission_failure sdk-filesystem "$path" "$((3*gib))" device - - 0;return 1;
    }
    bytes=$(measure "$path" $((3*gib)) '') || return 1
    apparent=$(measure "$path" $((3*gib)) --apparent-size) || return 1
    sdk_bytes=$((sdk_bytes+bytes));sdk_apparent=$((sdk_apparent+apparent))
  done
  ((sdk_bytes<=3*gib && sdk_apparent<=3*gib)) || {
    admission_failure sdk-aggregate "$job" "$((3*gib))" allocated-apparent - "$sdk_bytes/$sdk_apparent" 0;return 1;
  }
  dev=$(stat -c %d "$job");representative[$dev]=$job
  need[$dev]=$(( ${need[$dev]:-0}+3*gib-sdk_bytes ))
  if [[ ! -e $job/proof/cold560-packages.done ]]; then
    dev=$(stat -c %d /);representative[$dev]=/
    need[$dev]=$(( ${need[$dev]:-0}+gib ))
  else
    [[ ! -L $job/proof/cold560-packages.done && $(cat "$job/proof/cold560-packages.done") == "$GITHUB_SHA" ]] || {
      admission_failure packages-source-marker "$job/proof/cold560-packages.done" 0 source-binding - - 0;return 1;
    }
  fi
  for dev in "${!need[@]}"; do
    available=$(free_bytes "${representative[$dev]}") || {
      admission_failure free-bytes-read "${representative[$dev]}" "$((need[$dev]+reserve))" free - - 0;return 1;
    }
    printf '%s\t%s\t%s\t%s\n' "$EPOCHREALTIME" "$dev" "$available" "$((need[$dev]+reserve))" >> "$job/proof/cold560-admission.tsv" || {
      admission_failure capacity-record "${representative[$dev]}" "$((need[$dev]+reserve))" free - "$available" 0;return 1;
    }
    ((available>=need[$dev]+reserve)) || {
      admission_failure free-capacity "${representative[$dev]}" "$((need[$dev]+reserve))" free - "$available" 0;return 1;
    }
  done
}
source_final() {
  local status=0
  git ls-files -z > "$job/proof/index-after.bin" || status=1
  cmp "$job/proof/index-before.bin" "$job/proof/index-after.bin" || status=1
  tar --null --no-recursion --verbatim-files-from -T "$job/proof/index-before.bin" -cf - --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner |
    sha256sum > "$job/proof/source-after.sha256" || status=1
  cmp "$job/proof/source-before.sha256" "$job/proof/source-after.sha256" || status=1
  git diff --exit-code || status=1;git diff --cached --exit-code || status=1
  if [[ ${1:-} != without-admission ]];then admit || status=1;fi
  return "$status"
}
if [[ $mode == admit ]]; then admit;exit;fi
if [[ $mode == final-source ]]; then source_final;exit;fi
[[ ${CI_PREPARED_RUST_ROOT:-} == "$sdk" && -f ${CI_PREPARED_RUST_MANIFEST:?} ]] || fail 'prepared SDK missing'
[[ ! -e $out && ! -L $out && ! -e $build && ! -L $build && ! -e $cache && ! -L $cache ]] || fail 'fresh output/cache required'
mkdir "$out" "$out/logs" "$out/providers" "$out/inputs" "$out/licenses" "$build" "$build/tmp" "$build/audit-target"
deadline=$((SECONDS+1200));active= identity=;owned_code= owned_waited=;passed=false;observer=false;audit=false;protocol_complete=false;marker_controls_complete=false;established=false;groups_ok=true
declare -A held_fd=() held_id=() held_sha=() aliases=() alias_id=() bound=()
declare -a trees=() tree_names=()
stamp() { stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$1"; }
hold() {
  local path=$1 canonical before fd hash
  canonical=$(readlink -e "$path") || return 1
  [[ -f $canonical && ! -L $canonical && $(stat -c %s "$canonical") -le 536870912 ]] || return 1
  before=$(stamp "$canonical") || return 1
  if [[ -v aliases[$path] ]]; then
    [[ ${aliases[$path]} == "$canonical" && ${alias_id[$path]} == "$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path")" ]] || return 1
  else aliases[$path]=$canonical;alias_id[$path]=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path");fi
  [[ ! -v held_fd[$canonical] ]] || { [[ ${held_id[$canonical]} == "$before" ]];return; }
  exec {fd}<"$canonical" || return 1
  [[ $(stamp "/proc/$$/fd/$fd") == "$before" ]] || return 1
  hash=$(sha256sum "/proc/$$/fd/$fd") || return 1;hash=${hash%% *}
  [[ $(stamp "$canonical") == "$before" && $(stamp "/proc/$$/fd/$fd") == "$before" ]] || return 1
  held_fd[$canonical]=$fd;held_id[$canonical]=$before;held_sha[$canonical]=$hash
  printf '%s\t%s\t%s\n' "$canonical" "$before" "$hash" >> "$out/held-before.tsv"
}
verify_held() {
  local path fd hash status=0
  for path in "${!aliases[@]}";do
    [[ $(readlink -e "$path") == "${aliases[$path]}" && $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") == "${alias_id[$path]}" ]] || status=1
  done
  for path in "${!held_fd[@]}";do
    fd=${held_fd[$path]}
    [[ $(stamp "$path") == "${held_id[$path]}" && $(stamp "/proc/$$/fd/$fd") == "${held_id[$path]}" ]] || { status=1;continue; }
    hash=$(sha256sum "/proc/$$/fd/$fd") || { status=1;continue; };hash=${hash%% *}
    [[ $hash == "${held_sha[$path]}" && $(stamp "$path") == "${held_id[$path]}" ]] || status=1
    printf '%s\t%s\t%s\n' "$path" "${held_id[$path]}" "$hash" >> "$out/held-after.tsv" || status=1
  done
  return "$status"
}
tree() {
  find -P "$1" -path "$source/.git" -prune -o -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$2.roster" || return 1
  find -P "$1" -path "$source/.git" -prune -o -type f -print0 | sort -z | xargs -0 -r sha256sum > "$2.sha256" || return 1
  find -P "$1" -path "$source/.git" -prune -o -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$2.stable" || return 1
  cmp "$2.roster" "$2.stable"
}
add_tree() { tree_names+=("$1");trees+=("$2");tree "$2" "$out/$1.before"; }
bind_elf() {
  local path=$1 canonical line resolved interp index
  hold "$path" || return 1;canonical=$(readlink -e "$path")
  [[ ! -v bound[$canonical] ]] || return 0
  bound[$canonical]=true;index=${#bound[@]}
  if [[ $(head -c 4 "$canonical" | od -An -tx1 | tr -d ' \n') != 7f454c46 ]];then
    IFS= read -r line < "$canonical" || return 1
    case $line in '#!/bin/sh'|'#! /bin/sh'|'#!/usr/bin/sh') bind_elf /bin/sh;; '#!/bin/bash'|'#!/usr/bin/bash') bind_elf /bin/bash;; '#!/usr/bin/perl') bind_elf /usr/bin/perl;; *) return 1;;esac
    return
  fi
  readelf -h -l "$canonical" > "$out/providers/$index.elf" || return 1
  grep -Eq 'Class: +ELF64$' "$out/providers/$index.elf" && grep -Eq 'Machine: +Advanced Micro Devices X86-64$' "$out/providers/$index.elf" || return 1
  interp=$(sed -n 's/.*Requesting program interpreter: \([^]]*\)\].*/\1/p' "$out/providers/$index.elf")
  [[ -z $interp || $interp == /lib64/ld-linux-x86-64.so.2 ]] || return 1
  /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout 30s /usr/bin/ldd "$canonical" > "$out/providers/$index.ldd" 2> "$out/providers/$index.stderr" || return 1
  [[ ! -s $out/providers/$index.stderr ]] || return 1
  while IFS= read -r line;do
    [[ $line =~ ^[[:space:]]*linux-vdso.so.1[[:space:]]+\(0x[0-9a-f]+\)$ ]] && continue
    if [[ $line =~ ^[[:space:]]*[^[:space:]]+[[:space:]]+=\>[[:space:]]+(/[^[:space:]]+)[[:space:]]+\(0x[0-9a-f]+\)$ ]];then resolved=${BASH_REMATCH[1]}
    elif [[ $line =~ ^[[:space:]]*(/[^[:space:]]+)[[:space:]]+\(0x[0-9a-f]+\)$ ]];then resolved=${BASH_REMATCH[1]}
    else return 1;fi
    hold "$resolved" || return 1
    printf '%s\t%s\n' "$canonical" "$resolved" >> "$out/providers.tsv"
  done < "$out/providers/$index.ldd"
}
pid_identity() {
  local row rest;local -a fields
  { IFS= read -r -n 8193 row < "/proc/$1/stat"; } 2>/dev/null || return 1
  rest=${row##*) };read -r -a fields <<< "$rest"
  [[ ${#row} -le 8192 && $row == "$1 ("*") "* && ${#fields[@]} -ge 20 && ${fields[0]} != Z && ${fields[0]} != X &&
     ${fields[1]} == "$$" && ${fields[2]} == "$1" && ${fields[3]} == "$1" && ${fields[19]} =~ ^[0-9]+$ &&
     ( -z ${2:-} || ${fields[0]} == "$2" ) ]] || return 1
  printf '%s\n' "${fields[19]}"
}
kill_owned() { [[ -n $active && -n $identity && $(pid_identity "$active") == "$identity" ]] && kill -KILL -- "-$active"; }
group_failure() {
  local report=$1 kind=$2 code=$3 pgid=$4 exempt=$5 entry=$6 row=$7 disappeared=$8 rest exe_code=0
  local -a fields=()
  rest=${row##*) };read -r -a fields <<< "$rest" || :
  # 최초 실패만 create-new/64KiB에 보존한다. proc comm/raw stat와 exe 외 argv/env는 읽지 않는다.
  (
    set -o noclobber;ulimit -f 64 || exit 1
    {
      printf 'schema=1\nkind=%s\nreturn_code=%s\nexpected_pgid=%s\nexempt_pid=%s\nentry=%s\ndisappeared_before_failure=%s\n' \
        "$kind" "$code" "$pgid" "$exempt" "$entry" "$disappeared" || exit 1
      printf 'state=%s\nppid=%s\npgid=%s\nsession=%s\nstartticks=%s\nraw_stat_bytes=%s\nraw_stat=%s\n' \
        "${fields[0]:-unavailable}" "${fields[1]:-unavailable}" "${fields[2]:-unavailable}" \
        "${fields[3]:-unavailable}" "${fields[19]:-unavailable}" "${#row}" "$row" || exit 1
      printf 'exe=' || exit 1
      /usr/bin/readlink -e -- "${entry%/stat}/exe" 2>/dev/null || exe_code=$?
      printf '\nexe_read_status=%s\n' "$exe_code" || exit 1
    } > "$report"
  )
}
group_quiet() {
  local pgid=$1 exempt=${2:-} report=$3 entry row rest pid disappeared=0 seen=0
  local -a fields
  for entry in /proc/[0-9]*/stat;do
    row=;fields=();pid=${entry#/proc/};pid=${pid%/stat};seen=$((seen+1))
    if ! { IFS= read -r -n 8193 row < "$entry"; } 2>/dev/null;then
      if [[ ! -e $entry && ! -d ${entry%/stat} ]];then disappeared=$((disappeared+1));continue;fi
      group_failure "$report" unreadable-stat 2 "$pgid" "$exempt" "$entry" "$row" "$disappeared" || return 2
      return 2
    fi
    rest=${row##*) };read -r -a fields <<< "$rest"
    if [[ ${#row} -gt 8192 || $row != "$pid ("*") "* || ${#fields[@]} -lt 20 ||
          ! ${fields[0]} =~ ^[A-Za-z]$ || ! ${fields[1]} =~ ^[0-9]+$ || ! ${fields[2]} =~ ^[0-9]+$ ||
          ! ${fields[3]} =~ ^[0-9]+$ || ! ${fields[19]} =~ ^[0-9]+$ ]];then
      group_failure "$report" malformed-stat 2 "$pgid" "$exempt" "$entry" "$row" "$disappeared" || return 2
      return 2
    fi
    if [[ ${fields[2]} == "$pgid" && $pid != "$exempt" && ${fields[0]} != Z && ${fields[0]} != X ]];then
      group_failure "$report" live-member 1 "$pgid" "$exempt" "$entry" "$row" "$disappeared" || return 2
      return 1
    fi
  done
  (ulimit -f 64 || exit 1;printf '%s\t%s\t%s\t%s\t%s\n' "$pgid" "$exempt" "$seen" "$disappeared" "$EPOCHREALTIME" >> "$out/group-scans.tsv") || return 2
}
await_stopped() {
  local leader=$1 start=$2 end=$3 count
  for ((count=0;count<300;count++));do
    [[ $(pid_identity "$leader") == "$start" ]] || return 1
    [[ $(pid_identity "$leader" T) != "$start" ]] || return 0
    ((SECONDS<end)) || return 1
    sleep 0.01
  done
  return 1
}
provider_metadata() {
  local phase=$1
  (ulimit -f 64; /usr/bin/uname -rvm) > "$out/provider-$phase.kernel" || return 1
  (ulimit -f 64; /usr/bin/cat /proc/version) > "$out/provider-$phase.proc-version" || return 1
  (ulimit -f 64; /usr/bin/dpkg-query -W '-f=binary=${binary:Package}\nversion=${Version}\narchitecture=${Architecture}\nsource=${source:Package}\nsource_version=${source:Version}\n' strace) > "$out/provider-$phase.package" || return 1
  (ulimit -f 64; /usr/bin/strace --version) > "$out/provider-$phase.version" 2> "$out/provider-$phase.version.stderr" || return 1
}
marker_rejection() { printf 'cold tracer 560 marker: %s\n' "$1" >&2;return 1; }
verify_marker_receipt() {
  local log=$1 receipt=$2 trace=$3 parsed=$4 path n hash
  local -a paths=("$log" "$receipt" "$trace") identities=()
  for path in "${paths[@]}";do
    [[ -f $path && ! -L $path && $(readlink -e "$path") == "$path" &&
       $(stat -c '%u:%h' "$path") == "$(id -u):1" ]] || { marker_rejection input-binding;return 1; }
    identities+=("$(stamp "$path")") || { marker_rejection input-binding;return 1; }
  done
  [[ $(stat -c %s "$receipt") -le 65536 && ! -e $parsed && ! -L $parsed ]] || { marker_rejection input-binding;return 1; }
  # 허용한 두 prefix 외 marker는 무시하지 않고 거부한다. JSON payload는 단 한 줄/한 개다.
  (set -o noclobber;ulimit -f 64 || exit 1;awk '
    BEGIN {
      marker="PREPARED_COLD_BOOTSTRAP_AUDIT_JSON";
      bare=marker " ";
      progress="test prepared_audit::prepared_audit_actual_cold_bootstrap_receipt ... " bare
    }
    index($0,marker) {
      count++;
      if(index($0,bare)==1) payload=substr($0,length(bare)+1);
      else if(index($0,progress)==1) payload=substr($0,length(progress)+1);
      else {bad=1;next}
      if(length(payload)==0||length(payload)>65535) {bad=1;next}
      print payload
    }
    END {if(count!=1||bad)exit 1}
  ' "$log" > "$parsed") || { marker_rejection marker-shape;return 1; }
  hash=$(sha256sum "$trace") || { marker_rejection input-binding;return 1; };hash=${hash%% *}
  jq -se --slurpfile receipt "$receipt" --arg source "$GITHUB_SHA" --arg trace "$hash" '
    length==1 and ($receipt|length)==1 and (.[0]|type)=="object" and
    .[0]==$receipt[0] and .[0].audit_schema=="openpilot.prepared-cold-bootstrap.native.v1" and
    .[0].source_commit==$source and .[0].trace_sha256==$trace
  ' "$parsed" > /dev/null || { marker_rejection json-binding;return 1; }
  for n in "${!paths[@]}";do
    [[ ! -L ${paths[$n]} && $(readlink -e "${paths[$n]}") == "${paths[$n]}" &&
       $(stamp "${paths[$n]}") == "${identities[$n]}" ]] || { marker_rejection input-binding;return 1; }
  done
}
marker_receipt_controls() {
  local dir=$out/marker-controls name payload expected_code expected_reason code hash
  local marker='PREPARED_COLD_BOOTSTRAP_AUDIT_JSON '
  local progress='test prepared_audit::prepared_audit_actual_cold_bootstrap_receipt ... '
  [[ ! -e $dir && ! -L $dir ]] || fail 'fresh marker controls required'
  mkdir "$dir"
  printf 'authored-marker-control\n' > "$dir/base.trace"
  hash=$(sha256sum "$dir/base.trace");hash=${hash%% *}
  jq -n --arg source "$GITHUB_SHA" --arg trace "$hash" '{
    audit_schema:"openpilot.prepared-cold-bootstrap.native.v1",source_commit:$source,
    absolute_cargo:"/authored/sdk/bin/cargo",absolute_rustc:"/authored/sdk/bin/rustc",
    runner_environment:"github-hosted",runner_name:"authored-marker-control",
    exec_attempts:3,observed_rustup_exec_attempts:0,observed_python_exec_attempts:0,
    successful_absolute_cargo_execs:1,successful_absolute_rustc_execs:1,failed_exec_attempts:0,
    trace_sha256:$trace,manifest_sha256:("a"*64),manifest_regular_files_verified:1
  }' > "$dir/base.receipt.json"
  for name in bare progress missing duplicate foreign-prefix malformed multiple-json source trace whole-receipt;do
    ((SECONDS<deadline)) || fail 'marker controls exceeded shared deadline'
    cp "$dir/base.receipt.json" "$dir/$name.receipt.json"
    cp "$dir/base.trace" "$dir/$name.trace"
    payload=$(jq -c . "$dir/$name.receipt.json")
    expected_code=1;expected_reason=json-binding
    case $name in
      bare) printf '%s%s\n' "$marker" "$payload" > "$dir/$name.stdout";expected_code=0;expected_reason=;;
      progress) printf '%s%s%s\n' "$progress" "$marker" "$payload" > "$dir/$name.stdout";expected_code=0;expected_reason=;;
      missing) printf 'ok\n' > "$dir/$name.stdout";expected_reason=marker-shape;;
      duplicate) printf '%s%s\n%s%s%s\n' "$marker" "$payload" "$progress" "$marker" "$payload" > "$dir/$name.stdout";expected_reason=marker-shape;;
      foreign-prefix) printf 'test foreign::test ... %s%s\n' "$marker" "$payload" > "$dir/$name.stdout";expected_reason=marker-shape;;
      malformed) printf '%s{"source_commit":\n' "$marker" > "$dir/$name.stdout";;
      multiple-json) printf '%s%s {}\n' "$marker" "$payload" > "$dir/$name.stdout";;
      source|trace)
        # payload와 receipt가 함께 틀려도 현재 source/실제 trace binding에서 거부해야 한다.
        if [[ $name == source ]];then
          jq '.source_commit=("f"*40)' "$dir/base.receipt.json" > "$dir/$name.receipt.json"
        else
          jq '.trace_sha256=("f"*64)' "$dir/base.receipt.json" > "$dir/$name.receipt.json"
        fi
        payload=$(jq -c . "$dir/$name.receipt.json")
        printf '%s%s\n' "$marker" "$payload" > "$dir/$name.stdout";;
      whole-receipt)
        payload=$(jq -c '.exec_attempts+=1' "$dir/$name.receipt.json")
        printf '%s%s\n' "$marker" "$payload" > "$dir/$name.stdout";;
    esac
    code=0
    verify_marker_receipt "$dir/$name.stdout" "$dir/$name.receipt.json" "$dir/$name.trace" "$dir/$name.parsed.json" 2> "$dir/$name.stderr" || code=$?
    [[ $code == "$expected_code" ]] || fail "marker control exit differs: $name"
    if [[ $expected_code == 0 ]];then
      [[ ! -s $dir/$name.stderr ]] || fail "positive marker control stderr: $name"
    else
      [[ $(grep -Fxc "cold tracer 560 marker: $expected_reason" "$dir/$name.stderr") == 1 ]] || fail "marker control rejection differs: $name"
    fi
    printf '%s\t%s\t%s\tPASS\n' "$name" "$code" "${expected_reason:-accepted}" >> "$out/marker-receipt-controls.tsv"
  done
  [[ $(wc -l < "$out/marker-receipt-controls.tsv") == 10 ]] || fail 'exact marker control count differs'
  marker_controls_complete=true
  admit
}
final_admission_trace() {
  # reader에만 64KiB fsize를 건다. admit의 기존 출력 파일/cap에는 전파하지 않는다.
  local trace=$out/final-admission.xtrace result=$out/final-admission.exit reader_result=$out/final-admission.reader
  local path trace_fd reader writer_code=0 reader_code=0 open_code=0 value code
  final_admission_status=125;final_admission_diagnostic_complete=false
  for path in "$trace" "$result" "$reader_result";do
    [[ ! -e $path && ! -L $path ]] || return 1
  done
  exec {trace_fd}> >(
    set -o noclobber
    ulimit -f 64 || exit 125
    head -c 65536 > "$trace" || exit 125
    value=
    if IFS= read -r -N 1 value;then
      printf 'overflow\n' > "$reader_result"
      exit 1
    else
      code=$?
      [[ $code == 1 && -z $value ]] || exit 125
      printf 'complete\n' > "$reader_result" || exit 125
    fi
  ) || open_code=$?
  reader=$!
  if [[ $open_code != 0 ]];then
    wait "$reader" || :
    return 1
  fi
  (
    BASH_XTRACEFD=$trace_fd
    PS4='+cold560-final:${LINENO}:${FUNCNAME[0]:-main}:previous=$?: '
    set -x
    code=0;admit || code=$?
    set +x
    (set -o noclobber;printf '%s\n' "$code" > "$result") || exit 125
    exit "$code"
  ) || writer_code=$?
  exec {trace_fd}>&-
  wait "$reader" || reader_code=$?
  final_admission_status=$writer_code
  for path in "$trace" "$result" "$reader_result";do
    [[ -f $path && ! -L $path && $(readlink -e "$path") == "$path" &&
       $(stat -c '%u:%h' "$path") == "$(id -u):1" ]] || return 1
  done
  [[ $(stat -c %s "$trace") -le 65536 && $(stat -c %s "$result") -le 4 &&
     $(stat -c %s "$reader_result") == 9 && $(cat "$reader_result") == complete && $reader_code == 0 ]] || return 1
  value=$(cat "$result")
  [[ $value =~ ^(0|[1-9][0-9]{0,2})$ && $(stat -c %s "$result") == $((${#value}+1)) &&
     $value == "$writer_code" ]] || return 1
  awk '
    NR==1 && $0!~/^\+cold560-final:[1-9][0-9]*:final_admission_trace:previous=0: code=0$/ {bad=1}
    {last=$0}
    END {if(bad||NR<2||last!~/^\+cold560-final:[1-9][0-9]*:final_admission_trace:previous=[0-9]+: set \+x$/)exit 1}
  ' "$trace" || return 1
  final_admission_diagnostic_complete=true
}
finish() {
  local original=$? final=0 nonlive=$groups_ok seals=false n status wait_code final_admission_status=125 final_admission_diagnostic_complete=false
  trap - EXIT INT TERM HUP;set +e
  # aggregate 실패를 보존하면서 어느 final stage가 실패했는지 작은 원문 status로 분리한다.
  local finalizer_log_ready=false
  (set -o noclobber;printf 'stage\tstatus\n' > "$out/finalizer-stages.tsv") && finalizer_log_ready=true || final=1
  finalizer_stage() {
    $finalizer_log_ready && (ulimit -f 64 || exit 1;printf '%s\t%s\n' "$1" "$2" >> "$out/finalizer-stages.tsv")
  }
  if [[ -n $active ]];then
    status=0;kill_owned || status=$?
    finalizer_stage owned-kill "$status" || final=1
    if [[ $status == 0 ]];then
      wait_code=0;wait -f "$active" || wait_code=$?
      finalizer_stage owned-direct-wait "$wait_code" || final=1
    else
      final=1;nonlive=false
      finalizer_stage owned-direct-wait skipped-unverified-owner || final=1
    fi
    status=0;group_quiet "$active" '' "$out/logs/finalizer.group-failure" || status=$?
    finalizer_stage owned-group "$status" || final=1
    [[ $status == 0 ]] || { final=1;nonlive=false; }
    active= identity=
  fi
  status=0;cd "$source" || status=$?
  finalizer_stage source-cwd "$status" || final=1
  [[ $status == 0 ]] || final=1
  if $established;then
    status=0;provider_metadata after || status=$?
    finalizer_stage provider-metadata "$status" || final=1
    [[ $status == 0 ]] || final=1
    for n in kernel proc-version package version version.stderr;do
      status=0;cmp "$out/provider-before.$n" "$out/provider-after.$n" || status=$?
      finalizer_stage "provider-$n" "$status" || final=1
      [[ $status == 0 ]] || final=1
    done
  fi
  status=0;verify_held || status=$?
  finalizer_stage held-fd-path-hash "$status" || final=1
  [[ $status == 0 ]] || final=1
  for n in "${!trees[@]}";do
    status=0
    tree "${trees[$n]}" "$out/${tree_names[$n]}.after" &&
      cmp "$out/${tree_names[$n]}.before.roster" "$out/${tree_names[$n]}.after.roster" &&
      cmp "$out/${tree_names[$n]}.before.sha256" "$out/${tree_names[$n]}.after.sha256" || status=$?
    finalizer_stage "tree-${tree_names[$n]}" "$status" || final=1
    [[ $status == 0 ]] || final=1
  done
  status=0;source_final without-admission || status=$?
  finalizer_stage source-index "$status" || final=1
  [[ $status == 0 ]] || final=1
  status=0;final_admission_trace || status=$?
  finalizer_stage resource-admission-diagnostic "$status" || final=1
  [[ $status == 0 ]] || final=1
  status=$final_admission_status
  finalizer_stage resource-admission "$status" || final=1
  [[ $status == 0 ]] || final=1
  [[ $established == true && $final == 0 ]] && seals=true
  jq -n --arg source "$GITHUB_SHA" --arg phase "$phase" --argjson code "$original" --argjson final "$final" \
    --argjson seals "$seals" --argjson nonlive "$nonlive" --argjson observer "$observer" --argjson audit "$audit" --argjson protocol "$protocol_complete" --argjson markers "$marker_controls_complete" --argjson admission_diagnostic "$final_admission_diagnostic_complete" --argjson passed "$passed" \
    '{schema:1,source:$source,phase:$phase,command_exit:$code,finalizer_exit:$final,immutable_seals:$seals,owned_processes_nonlive:$nonlive,
      observer_complete:$observer,strict_cold_audit:$audit,protocol_controls_complete:$protocol,protocol_controls_expected:3,
      marker_receipt_controls_complete:$markers,marker_receipt_controls_expected:10,final_admission_diagnostic_complete:$admission_diagnostic,
      pass:($code==0 and $final==0 and $seals and $nonlive and $observer and $audit and $protocol and $markers and $admission_diagnostic and $passed),
      diagnostic_provider:true,ubuntu_elf_identical:false,root_fixed:false,nonreproduction_is_fix:false,additional_ptrace_requests:0,
      all_adopted_children_reaped:false,closed_dso:false,measured_peak_guarantee:false,retirement_credit:0}' > "$out/outcome.json" || { final=1;seals=false; }
  printf 'immutable_seals=%s\nowned_processes_nonlive=%s\n' "$seals" "$nonlive" >> "$GITHUB_OUTPUT" || final=1
  [[ $original == 0 && $final == 0 && $passed == true ]] || exit 1
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
base_env=(PATH="$sdk/bin:/usr/bin:/bin" HOME="$job/home" LC_ALL=C TZ=UTC TMPDIR="$build/tmp" RUNNER_TEMP="$job"
  CARGO_HOME="$job/cargo-home" RUSTUP_HOME="$job/rustup-home" RUSTUP_TOOLCHAIN=1.94.0
  RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
  GITHUB_ACTIONS=true GITHUB_REPOSITORY=bin9208/openpilot-rust RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux RUNNER_ARCH=X64
  GITHUB_RUN_ID="$GITHUB_RUN_ID" GITHUB_RUN_ATTEMPT="$GITHUB_RUN_ATTEMPT" GITHUB_JOB="$GITHUB_JOB" GITHUB_SHA="$GITHUB_SHA" RUNNER_NAME="$RUNNER_NAME"
  CI_JOB_TEMP="$job" GITHUB_WORKSPACE="$source" CI_PREPARED_RUST_ROOT="$sdk" CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST"
  CARGO_TERM_COLOR=never GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1)
extra_env=()
# 명시 run-inherited에서만 사용한다. 기존 run의 launcher/observer 경로는 유지한다.
launch_label= launch_dir= launch_fault= launch_seen=false
launch_tracer= launch_parent=
declare -a launch_strip=()
approve_launch() {
  local record tag schema pid start dev ino parent pstart pdev pino source_sha helper_sha provider_sha extra
  [[ $mode == run-inherited && -n $launch_label && $phase == "$launch_label" && $launch_seen == false ]] || return 0
  [[ -e $launch_dir/ready || -L $launch_dir/ready ]] || return 0
  record=$(/usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout --signal=KILL 5s "$fixture" ready "$launch_dir") || fail 'launch request reader'
  read -r tag schema pid start dev ino parent pstart pdev pino source_sha helper_sha provider_sha extra <<< "$record"
  [[ $tag == COLD560_LAUNCH && $schema == 1 && -z $extra && $pid =~ ^[1-9][0-9]*$ && $parent =~ ^[1-9][0-9]*$ &&
     $source_sha == "$GITHUB_SHA" && $helper_sha == "$COLD560_HELPER_SHA256" && $provider_sha == "${held_sha[$tracer]}" ]] || fail 'launch request binding'
  # 실제 STOP한 tracer/timeout의 ELF를 이미 열린 provider FD의 full SHA/identity와 연결한다.
  [[ $(readlink -e "/proc/$pid/exe") == "$tracer" && $(stamp "/proc/$pid/exe") == "${held_id[$tracer]}" &&
     $(readlink -e "/proc/$parent/exe") == "$(readlink -e /usr/bin/timeout)" &&
     $(stamp "/proc/$parent/exe") == "${held_id[$(readlink -e /usr/bin/timeout)]}" ]] || fail 'live launch ELF identity'
  printf '%s  %s\n' "${held_sha[$tracer]}" "/proc/$pid/exe" \
    "${held_sha[$(readlink -e /usr/bin/timeout)]}" "/proc/$parent/exe" | sha256sum --check --strict --quiet - || fail 'live launch ELF SHA'
  /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout --signal=KILL 5s "$fixture" approve "$launch_dir" \
    "$active" "$identity" "$(stat -c '%d:%i' "$tracer")" "$(stat -Lc '%d:%i' /usr/bin/timeout)" \
    "$launch_dir/tracer.argv.nul" "$launch_dir/timeout.argv.nul" "$GITHUB_SHA" "$COLD560_HELPER_SHA256" "${held_sha[$tracer]}" strict "$launch_fault" \
    > "$launch_dir/approval.stdout" 2> "$launch_dir/approval.stderr" || fail 'fresh launch approval refused'
  [[ $(stat -c %s "$launch_dir/approval.stdout") -le 4096 && $(stat -c %s "$launch_dir/approval.stderr") -le 4096 &&
     $(pid_identity "$active") == "$identity" && $(stamp "/proc/$pid/exe") == "${held_id[$tracer]}" ]] || fail 'launch changed before CONT'
  (set -o noclobber;ulimit -f 4
    jq -n --arg source "$GITHUB_SHA" --arg helper "$COLD560_HELPER_SHA256" --arg provider "${held_sha[$tracer]}" \
      --arg request "$record" --arg leader "$active" --arg leader_start "$identity" --arg fault "$launch_fault" \
      '{schema:1,source:$source,helper_sha256:$helper,provider_sha256:$provider,request:$request,
        owned_leader:$leader,owned_leader_start:$leader_start,approval_fault:$fault,
        actual_timeout_parent_and_exact_argv_verified:true,parent_link_observed:false}' > "$launch_dir/binding.json"
  ) || fail 'bounded launch binding receipt'
  # ready/argv/approval은 아직 tracee startup 전이다. proc start는 approver가 두 번 대조했다.
  launch_tracer=$pid;launch_parent=$parent;launch_seen=true
  kill -CONT "$pid" || fail 'fresh tracer CONT'
}
inherited_command() {
  local label=$1 seconds=$2 expectation=$3 fault=$4 path
  shift 4
  local -a saved_env=("${extra_env[@]}")
  launch_label=$label;launch_fault=$fault;launch_seen=false;launch_tracer=;launch_parent=
  launch_dir=$out/inheritance/$label
  mkdir -- "$launch_dir"
  local value=1
  [[ $expectation != bad-mode ]] || value=2
  extra_env+=(COLD560_FILTER_INHERIT="$value" COLD560_LAUNCH_DIR="$launch_dir" COLD560_SOURCE_SHA="$GITHUB_SHA"
    COLD560_HELPER_SHA="$COLD560_HELPER_SHA256" COLD560_PROVIDER_SHA="${held_sha[$tracer]}")
  start_owned "$label" "$seconds" "$@"
  group_quiet "$active" "$active" "$out/logs/$label.group-before-release" || fail 'inherited command live/unknown group'
  complete_owned "$label"
  if [[ $expectation == pass ]];then
    [[ $owned_code == 0 && $launch_seen == true ]] || fail "$label inherited invocation failed"
    [[ -f $launch_dir/inherit.log && ! -L $launch_dir/inherit.log && $(stat -c '%u:%a:%h' "$launch_dir/inherit.log") == "$(id -u):600:1" &&
       $(stat -c %s "$launch_dir/inherit.log") -le 16384 ]] || fail 'inherited receipt shape/bound'
    # 고정 다섯 record와 정상 EOF를 함께 요구한다.
    awk -v tracer="$launch_tracer" -v parent="$launch_parent" '
      NR==1 {if($0!="COLD560_INHERIT start schema=1 tracer=" tracer " parent=" parent)bad=1;next}
      NR==2 {if($0!~/^COLD560_INHERIT root pid=[1-9][0-9]*$/)bad=1;split($3,a,"=");root=a[2];next}
      NR==3 {if($0!="COLD560_INHERIT root_seccomp pid=" root)bad=1;next}
      NR==4 {if($0!="COLD560_INHERIT root_exit pid=" root)bad=1;next}
      NR==5 {if(NF!=8||$1!="COLD560_INHERIT"||$2!="finish"||$3!="root=" root||$4!="root_seccomp=1"||$5!~/^fresh=[0-9]+$/||$6!~/^inherited=[0-9]+$/||$7!="additional_ptrace=0"||$8!="parent_link_observed=0")bad=1;split($5,a,"=");split($6,b,"=");if(a[2]!=b[2]||a[2]<0)bad=1;next}
      {bad=1} END {if(bad||NR!=5)exit 1}
    ' "$launch_dir/inherit.log" || fail 'inherited receipt health'
    for path in "$launch_dir/ready" "$launch_dir/approve" "$launch_dir/inherit.log" "$launch_dir/binding.json" "$launch_dir/approval.stdout" "$launch_dir/tracer.argv.nul" "$launch_dir/timeout.argv.nul";do hold "$path";done
  else
    [[ $owned_code == 1 && ! -s $out/logs/$label.stdout && ! -e $launch_dir/inherit.log && ! -L $launch_dir/inherit.log ]] || fail 'negative executed target/accepted context'
    if [[ $expectation == binding ]];then
      [[ $launch_seen == true ]] && grep -Fq 'COLD560_INHERIT launch approval' "$out/logs/$label.stderr" || fail 'wrong approval exact error'
    else
      [[ $launch_seen == false ]] && grep -Fq 'COLD560_INHERIT unsupported context' "$out/logs/$label.stderr" || fail 'unsupported context exact error'
    fi
    [[ ! -s $out/inheritance/$label.exec ]] || fail 'negative has target exec trace'
  fi
  extra_env=("${saved_env[@]}");launch_label=;launch_fault=;launch_dir=
  admit
}
inherited_controls() {
  mkdir "$out/inheritance" "$build/semantic"
  fixture=$build/semantic/cold560-exec-inheritance
  run inheritance-fixture-build 60 "$sdk/bin/rustc" --edition=2021 --crate-name cold560_exec_inheritance -C opt-level=2 \
    "$source/tools/ci/fixtures/cold560_exec_inheritance.rs" -o "$fixture"
  [[ ! -e $cache && ! -L $cache ]] || fail 'semantic fixture warmed cold cache'
  bind_elf "$fixture"
  launch_strip=(-E COLD560_LOG -E COLD560_FILTER_INHERIT -E COLD560_LAUNCH_DIR -E COLD560_SOURCE_SHA -E COLD560_HELPER_SHA -E COLD560_PROVIDER_SHA)
  local name label receipt
  local -a options=(--debug --seccomp-bpf -f -qq -s 65535 -v -xx -e trace=execve,execveat)
  for name in fork vfork thread nonleader execve execveat root-first;do
    label=inherit-$name;receipt=$out/inheritance/$label/inherit.log
    (set -o noclobber;printf '%s\0' "${base_env[@]}" > "$out/inheritance/$label.env.nul")
    extra_env=(COLD560_LOG="$out/inheritance/$label.observer")
    inherited_command "$label" 20 pass none "$tracer" "${options[@]}" "${launch_strip[@]}" \
      -o "$out/inheritance/$label.exec" "$fixture" scenario "$name" "$receipt"
    [[ $(stat -c %s "$out/inheritance/$label.observer") -le 65536 &&
       $(grep -Ec '^COLD560 finish .* io_error=0 overflow=0$' "$out/inheritance/$label.observer") == 1 ]] || fail 'semantic observer health'
    case $name in
      fork|vfork|thread|nonleader) grep -Eq ' fresh=[1-9][0-9]* inherited=[1-9][0-9]* ' "$receipt" || fail 'fresh child detail absent';;
      root-first) awk 'END {split($5,a,"=");if(a[2]<2)exit 1}' "$receipt" || fail 'root-first descendant coverage';;
    esac
    case $name in
      fork|vfork|root-first) printf 'COLD560_FIXTURE leaf %s\nCOLD560_FIXTURE done %s\n' "$name" "$name";;
      thread) printf 'COLD560_FIXTURE done thread\n';;
      *) printf 'COLD560_FIXTURE leaf %s\n' "$name";;
    esac > "$out/inheritance/$label.stdout.expected"
    cmp "$out/inheritance/$label.stdout.expected" "$out/logs/$label.stdout" || fail 'semantic exact stdout'
    hold "$out/inheritance/$label.exec";hold "$out/inheritance/$label.env.nul"
    extra_env=()
    run "verify-$label" 10 "$fixture" verify "$out/inheritance/$label.exec" "$name" "$fixture" "$out/inheritance/$label.env.nul" "$receipt"
    [[ $(grep -Ec '^COLD560_SEMANTIC .+ PASS$' "$out/logs/verify-$label.stdout") == 1 ]] || fail 'semantic exact marker'
    [[ $name != vfork ]] || grep -Eq ',EVENT_VFORK \([0-9]+\)' "$out/logs/$label.stderr" || fail 'actual VFORK event unobserved (HOLD)'
    printf '%s\tpositive\tPASS\n' "$name" >> "$out/inheritance/controls.tsv"
  done
  # 실제 fresh chain을 먼저 검증한 뒤 승인 한 필드만 오염한다. target stdout/exec는 모두0이다.
  for name in missing wrong-start wrong-parent wrong-provider;do
    label=inherit-bind-$name
    extra_env=(COLD560_LOG="$out/inheritance/$label.observer")
    inherited_command "$label" 10 binding "$name" "$tracer" "${options[@]}" "${launch_strip[@]}" \
      -o "$out/inheritance/$label.exec" "$fixture" leaf forbidden
    printf '%s\tnegative-binding\tPASS\n' "$name" >> "$out/inheritance/controls.tsv"
  done
  local -a varied=()
  for name in bad-mode attach mixed-attach daemon no-bpf no-follow pipe;do
    label=inherit-context-$name;varied=("${options[@]}")
    case $name in
      attach|mixed-attach) varied+=(-p "$$");;
      daemon) varied+=(-D);;
      no-bpf) varied=(--debug -f -qq -s 65535 -v -xx -e trace=execve,execveat);;
      no-follow) varied=(--debug --seccomp-bpf -qq -s 65535 -v -xx -e trace=execve,execveat);;
    esac
    extra_env=(COLD560_LOG="$out/inheritance/$label.observer")
    local output=$out/inheritance/$label.exec
    [[ $name != pipe ]] || output="|$fixture leaf forbidden"
    local -a target=("$fixture" leaf forbidden)
    [[ $name != attach ]] || target=()
    local expectation=context
    [[ $name != bad-mode ]] || expectation=bad-mode
    inherited_command "$label" 10 "$expectation" none "$tracer" "${varied[@]}" "${launch_strip[@]}" -o "$output" "${target[@]}"
    printf '%s\tnegative-context\tPASS\n' "$name" >> "$out/inheritance/controls.tsv"
  done
  extra_env=()
  [[ $(wc -l < "$out/inheritance/controls.tsv") == 18 && ! -e $cache && ! -L $cache ]] || fail 'inherited controls/count/cold-cache'
  hold "$out/inheritance/controls.tsv"
  jq -n --arg source "$GITHUB_SHA" --arg provider "${held_sha[$tracer]}" \
    '{schema:1,source:$source,provider_sha256:$provider,positive:7,negative_binding:4,negative_context:7,
      actual_vfork_event_required:true,additional_ptrace_requests:0,parent_link_observed:false,
      nommu_and_old_seccomp_order_host_controls_run:false,
      source_bound_fresh_timeout_launch:true,root_fixed:false,nonreproduction_is_fix:false,retirement_credit:0}' > "$out/inheritance/controls.json"
  hold "$out/inheritance/controls.json";verify_held
}

start_owned() {
  local label=$1 seconds=$2 leader ready go done release status count end remaining row
  shift 2;phase=$label;admit
  [[ -z $active && -z $identity && $label =~ ^[A-Za-z0-9_.-]+$ ]] || fail 'owned command overlap/label'
  remaining=$((deadline-SECONDS));((remaining>0)) || fail '1200s diagnostic deadline'
  ((seconds<=remaining)) || seconds=$remaining
  ready=$out/logs/$label.ready;go=$out/logs/$label.go;done=$out/logs/$label.done;release=$out/logs/$label.release;status=$out/logs/$label.status
  printf '%s\0' "$label" "$@" >> "$out/commands.nul"
  # command 종료 뒤 builtin STOP만 한다. release 대기용 외부 자식을 만들지 않는다.
  /usr/bin/setsid /usr/bin/bash --noprofile --norc -c '
    set -u;ulimit -c 0;ulimit -f 524288 || exit 125
    ready=$1;go=$2;done=$3;release=$4;status=$5;seconds=$6;shift 6
    printf "%s\n" "$$" > "$ready"
    for ((n=0;n<300;n++));do [[ ! -e $go ]] || break;sleep 0.01;done
    [[ -e $go ]] || exit 125
    code=0
    /usr/bin/timeout --foreground --signal=TERM --kill-after=15s "${seconds}s" "$@" || code=$?
    printf "%s\n" "$code" > "$status" || exit 125
    : > "$done" || exit 125
    kill -STOP "$$" || exit 125
    [[ -f $release && ! -L $release ]] || exit 125
    exit "$code"
  ' cold560-owned "$ready" "$go" "$done" "$release" "$status" "$seconds" /usr/bin/env -i "${base_env[@]}" "${extra_env[@]}" "$@" \
    > "$out/logs/$label.stdout" 2> "$out/logs/$label.stderr" < /dev/null &
  leader=$!;active=$leader;identity=
  for ((count=0;count<300;count++));do
    if [[ -s $ready && $(cat "$ready") == "$leader" ]];then identity=$(pid_identity "$leader") && break;fi
    sleep 0.01
  done
  [[ -n $identity ]] || fail 'owned leader handshake failed'
  if [[ $mode == run-inherited && $label == "$launch_label" ]];then
    (set -o noclobber;printf '%s\0' "$@" > "$launch_dir/tracer.argv.nul") || fail 'tracer argv create-new'
    (set -o noclobber;printf '%s\0' /usr/bin/timeout --foreground --signal=TERM --kill-after=15s "${seconds}s" /usr/bin/env -i \
      "${base_env[@]}" "${extra_env[@]}" "$@" > "$launch_dir/timeout.argv.nul") || fail 'timeout argv create-new'
  fi
  : > "$go";end=$((SECONDS+seconds+20))
  while [[ ! -e $done ]];do
    [[ $(pid_identity "$leader") == "$identity" ]] || fail 'leader disappeared before status'
    ((SECONDS<end)) || fail 'owned command deadline'
    if [[ $mode == run-inherited && $label == "$launch_label" ]];then approve_launch;fi
    admit || fail 'resource guard failed'
    sleep 0.5
  done
  await_stopped "$leader" "$identity" "$end" || fail 'owned leader did not reach stable T state'
  { IFS= read -r -n 8193 row < "/proc/$leader/stat"; } 2>/dev/null || fail 'stopped leader stat unreadable'
  [[ ${#row} -le 8192 && $(pid_identity "$leader" T) == "$identity" ]] || fail 'stopped leader identity differs'
  (set -o noclobber;printf '%s\n' "$row" > "$out/logs/$label.stopped.stat") || fail 'stopped leader proof failed'
}
complete_owned() {
  local label=$1 leader=$active code waited=0
  [[ $(pid_identity "$leader" T) == "$identity" ]] || fail 'leader not owned/stopped before release'
  IFS= read -r code < "$out/logs/$label.status"
  [[ $code =~ ^[0-9]+$ ]] && ((code<=255)) || fail 'command status malformed'
  (set -o noclobber;: > "$out/logs/$label.release") || fail 'release marker failed'
  [[ $(pid_identity "$leader" T) == "$identity" ]] || fail 'leader identity changed before CONT'
  kill -CONT "$leader" || fail 'owned CONT failed'
  wait -f "$leader" || waited=$?
  owned_code=$code;owned_waited=$waited
  active= identity=
  group_quiet "$leader" '' "$out/logs/$label.group-after-wait" || { groups_ok=false;fail 'group live/unknown after direct wait'; }
  printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$waited" "$EPOCHREALTIME" >> "$out/exit-codes.tsv"
  [[ $waited == "$code" ]] || fail "$label status/wait mismatch $code/$waited"
}
run() {
  local label=$1 seconds=$2
  shift 2
  start_owned "$label" "$seconds" "$@"
  group_quiet "$active" "$active" "$out/logs/$label.group-before-release" || fail 'command left live/unknown group members'
  complete_owned "$label"
  [[ $owned_code == 0 ]] || fail "$label exit $owned_code (wait $owned_waited)"
  admit
}
protocol_controls() {
  local label child child_start row rest group_code=0 waited=0 code count child_nonlive=false
  local -a fields
  # 모두 hosted에서만 실행한다. 합성 성공은 cold/errno/auditor의 실제 성공이 아니다.
  run protocol-childless 5 /usr/bin/bash --noprofile --norc -c 'exit 0'
  printf 'childless-stop-cont\t0\t0\tPASS\n' >> "$out/protocol-controls.tsv"
  label=protocol-exit7
  start_owned "$label" 5 /usr/bin/bash --noprofile --norc -c 'exit 7'
  group_quiet "$active" "$active" "$out/logs/$label.group-before-release" || fail 'nonzero control left live/unknown group'
  complete_owned "$label"
  [[ $owned_code == 7 && $owned_waited == 7 ]] || fail 'nonzero control lost original status'
  printf 'original-nonzero-direct-wait\t7\t7\tPASS\n' >> "$out/protocol-controls.tsv"
  label=protocol-live-member
  start_owned "$label" 5 /usr/bin/bash --noprofile --norc -c '
    set -eu
    /usr/bin/bash --noprofile --norc -c '\''printf "%s\n" "$$" > "$1";kill -STOP "$$";exit 0'\'' cold560-live-child "$1" &
    child=$!
    for ((n=0;n<300;n++));do
      if [[ -s $1 ]];then
        IFS= read -r value < "$1";[[ $value == "$child" ]] || exit 125
        IFS= read -r row < "/proc/$child/stat" || exit 125
        rest=${row##*) };read -r -a fields <<< "$rest"
        [[ ${#fields[@]} -ge 20 && ${fields[0]} == T && ${fields[1]} == "$$" ]] && exit 0
      fi
      sleep 0.01
    done
    exit 125
  ' cold560-live-control "$out/logs/$label.child"
  IFS= read -r code < "$out/logs/$label.status"
  [[ $code == 0 ]] || fail 'live control command did not complete'
  IFS= read -r child < "$out/logs/$label.child"
  [[ $child =~ ^[1-9][0-9]*$ && $child != "$active" ]] || fail 'live control child marker malformed'
  { IFS= read -r -n 8193 row < "/proc/$child/stat"; } 2>/dev/null || fail 'live control child absent'
  rest=${row##*) };read -r -a fields <<< "$rest"
  [[ ${#row} -le 8192 && $row == "$child ("*") "* && ${#fields[@]} -ge 20 &&
     ${fields[0]} == T && ${fields[2]} == "$active" && ${fields[3]} == "$active" && ${fields[19]} =~ ^[0-9]+$ ]] || fail 'live control child identity/state differs'
  child_start=${fields[19]}
  group_quiet "$active" "$active" "$out/logs/$label.group-before-release" || group_code=$?
  [[ $group_code == 1 ]] || fail 'live control did not reach exact live-member rejection'
  grep -Fx 'kind=live-member' "$out/logs/$label.group-before-release" >/dev/null
  grep -Fx "entry=/proc/$child/stat" "$out/logs/$label.group-before-release" >/dev/null
  kill_owned || fail 'live control owned cleanup failed'
  wait -f "$active" || waited=$?
  [[ $waited == 137 ]] || fail 'live control direct wait did not observe KILL'
  # 이 통제에서 KILL한 특정 자식의 kernel 종료 전이만 bounded하게 기다린다.
  # 일반 command의 live/unknown 검사에는 재시도/유예를 적용하지 않는다.
  for ((count=0;count<300;count++));do
    ((SECONDS<deadline)) || fail 'control cleanup exceeded shared deadline'
    if [[ ! -e /proc/$child/stat && ! -d /proc/$child ]];then child_nonlive=true;break;fi
    if ! { IFS= read -r -n 8193 row < "/proc/$child/stat"; } 2>/dev/null;then
      if [[ ! -e /proc/$child/stat && ! -d /proc/$child ]];then child_nonlive=true;break;fi
      fail 'killed control child stat unreadable'
    fi
    rest=${row##*) };read -r -a fields <<< "$rest"
    [[ ${#row} -le 8192 && $row == "$child ("*") "* && ${#fields[@]} -ge 20 &&
       ${fields[2]} == "$active" && ${fields[3]} == "$active" && ${fields[19]} == "$child_start" ]] || fail 'killed control child identity changed'
    if [[ ${fields[0]} == Z || ${fields[0]} == X ]];then child_nonlive=true;break;fi
    sleep 0.01
  done
  if ! $child_nonlive;then
    group_quiet "$active" '' "$out/logs/$label.group-cleanup-timeout" || :
    fail 'killed control child did not become nonlive'
  fi
  group_quiet "$active" '' "$out/logs/$label.group-after-kill" || { groups_ok=false;fail 'live control group not nonlive after KILL'; }
  active= identity=
  printf 'genuine-live-member-rejected\t1\t137\tPASS\n' >> "$out/protocol-controls.tsv"
  [[ $(wc -l < "$out/protocol-controls.tsv") == 3 ]] || fail 'exact protocol control count differs'
  protocol_complete=true
  admit
}
for tool in bash sh env git find sort xargs sha256sum stat readlink readelf sed awk grep head od tr cat cmp df du jq timeout setsid sleep curl gcc cc make as ar ranlib ld nm pkg-config dpkg-query dpkg-deb dpkg-buildflags perl tar xz patch cp chmod mkdir id dirname basename rm ldd wc;do
  path=$(type -P "$tool") || fail "provider missing: $tool";bind_elf "$path"
done
for tool in cargo rustc rustdoc;do bind_elf "$sdk/bin/$tool";done
bind_elf /usr/bin/strace
hold /etc/ld.so.cache;hold "$source/tools/ci/cold_tracer_diagnostic_560.sh"
for path in "$CI_PREPARED_RUST_MANIFEST" "$job/proof/index-before.bin" "$job/proof/source-before.sha256" "$job/proof/SOURCE_COMMIT" "$job/proof/ownership.txt";do hold "$path";done
patchfile=$source/tools/ci/fixtures/strace-6.8-560-observer.patch
printf '%s  %s\n' "${COLD560_PATCH_SHA256:?}" "$patchfile" | sha256sum --check --strict --quiet -
hold "$patchfile";cp "$patchfile" "$out/inputs/observer.patch"
if [[ $mode == run-inherited ]];then
  inherited_patch=$source/tools/ci/fixtures/strace-6.8-560-inherited-filter.patch
  fixture_source=$source/tools/ci/fixtures/cold560_exec_inheritance.rs
  printf '%s  %s\n' "${COLD560_INHERITED_PATCH_SHA256:?}" "$inherited_patch" "${COLD560_FIXTURE_SHA256:?}" "$fixture_source" | sha256sum --check --strict --quiet -
  hold "$inherited_patch";hold "$fixture_source"
  cp "$inherited_patch" "$out/inputs/inherited-filter.patch"
fi
printf '%s  %s\n' 28f957c227012de0b18d1bd7fff2d396cb693ea60ed8013be68de071e84b5001 /usr/bin/strace | sha256sum --check --strict --quiet -
add_tree source "$source";add_tree sdk "$sdk";add_tree headers /usr/include;add_tree gcc /usr/lib/gcc/x86_64-linux-gnu
add_tree perl-source /usr/share/perl;add_tree dpkg-perl /usr/share/perl5
for path in /usr/lib/x86_64-linux-gnu/perl /usr/lib/x86_64-linux-gnu/perl5;do
  [[ ! -d $path ]] || add_tree "perl-$(basename "$path")" "$path"
done
established=true
provider_metadata before
marker_receipt_controls
protocol_controls
run system-version 30 /usr/bin/strace --version
run kernel 30 /usr/bin/uname -rvm
run package 30 /usr/bin/dpkg-query -W '-f=binary=${binary:Package}\nversion=${Version}\nsource=${source:Package}\nsource_version=${source:Version}\n' strace
run flags-c 30 /usr/bin/env DEB_BUILD_MAINT_OPTIONS=hardening=+all /usr/bin/dpkg-buildflags --get CFLAGS
run flags-cpp 30 /usr/bin/env DEB_BUILD_MAINT_OPTIONS=hardening=+all /usr/bin/dpkg-buildflags --get CPPFLAGS
run flags-ld 30 /usr/bin/env DEB_BUILD_MAINT_OPTIONS=hardening=+all /usr/bin/dpkg-buildflags --get LDFLAGS
while IFS=$'\t' read -r name bytes digest;do
  run "download-$name" 120 /usr/bin/curl --fail --location --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 90 --max-filesize "$bytes" \
    --output "$out/inputs/$name" "https://archive.ubuntu.com/ubuntu/pool/main/s/strace/$name"
  [[ $(stat -c %s "$out/inputs/$name") == "$bytes" ]]
  printf '%s  %s\n' "$digest" "$out/inputs/$name" | sha256sum --check --strict --quiet -
  hold "$out/inputs/$name"
done <<'PINS'
strace_6.8.orig.tar.xz	2457948	ba6950a96824cdf93a584fa04f0a733896d2a6bc5f0ad9ffe505d9b41e970149
strace_6.8-0ubuntu2.debian.tar.xz	21776	6efc6fde478f3beb37dbd246f1e9fe4f13e74eef794b5b66d7589f0933bc351b
strace_6.8-0ubuntu2.dsc	2285	330679ec872f5d097809a28fe0bcd4fe3a0ba2a57e639feec53ffc3745aa831b
strace_6.8-0ubuntu2_amd64.deb	584172	d588810ae26b06fee6678dc81e5b54f6efcde8e718e4589adb4d11d254b9820b
PINS
run deb-member 30 /bin/bash -c 'set -euo pipefail; /usr/bin/dpkg-deb --fsys-tarfile "$1" | /usr/bin/tar -xOf - ./usr/bin/strace > "$2"' cold560-deb \
  "$out/inputs/strace_6.8-0ubuntu2_amd64.deb" "$out/inputs/ubuntu-strace.elf"
[[ $(stat -c %s "$out/inputs/ubuntu-strace.elf") == 2087432 ]]
cmp /usr/bin/strace "$out/inputs/ubuntu-strace.elf"
hold "$out/inputs/ubuntu-strace.elf"
run unpack-orig 60 /usr/bin/tar -xJf "$out/inputs/strace_6.8.orig.tar.xz" -C "$build" --no-same-owner --no-same-permissions
native=$build/strace-6.8
[[ -d $native && ! -L $native && $(readlink -e "$native") == "$native" ]]
run unpack-debian 30 /usr/bin/tar -xJf "$out/inputs/strace_6.8-0ubuntu2.debian.tar.xz" -C "$native" --no-same-owner --no-same-permissions
printf '02-disable_delay_test\n0007-XFAIL-netlink_crypto.patch\n0008-XFAIL-nlattr_crypto_user_alg.patch\nfix-time_t-conversion.patch\n' > "$out/quilt.expected"
cmp "$out/quilt.expected" "$native/debian/patches/series"
while IFS= read -r name;do
  run "quilt-$name" 30 /usr/bin/patch --directory="$native" --batch --fuzz=0 --no-backup-if-mismatch -p1 --input="$native/debian/patches/$name"
done < "$out/quilt.expected"
printf '%s  %s\n' 0d8c5449b011a8cd16f0b5d0864f57fa633867876682270fee294390bb0e62b1 "$native/src/syscall.c" \
  21fe0b653612b9354ff783a9ca1643d39682d85fd38bf32a14b09cdea41febd1 "$native/src/strace.c" \
  8073129838f921c5c0286c3c6cc0bebf7832b04f476340114ea25e5c8216c4a1 "$native/src/filter_seccomp.c" | sha256sum --check --strict --quiet -
run observer-patch 30 /usr/bin/patch --directory="$native" --batch --fuzz=0 --no-backup-if-mismatch -p1 --input="$patchfile"
printf '%s  %s\n' 41b9dffd742377a76215b75c6a9b1a76d63483493dca2afb50bac87e8877d777 "$native/src/syscall.c" \
  07397b01bc0bcea2c58aa4a8b5a1cfe71778f929934529155a7c35ae55cbd638 "$native/src/strace.c" | sha256sum --check --strict --quiet -
if [[ $mode == run-inherited ]];then
  run inherited-patch 30 /usr/bin/patch --directory="$native" --batch --fuzz=0 --no-backup-if-mismatch -p1 --input="$inherited_patch"
  printf '%s  %s\n' ef7b41e35c2cdc4ddf3b7c74348e9e36d298f4e6dd137f20f1e42054802dd75b "$native/src/strace.c" \
    41b9dffd742377a76215b75c6a9b1a76d63483493dca2afb50bac87e8877d777 "$native/src/syscall.c" \
    8073129838f921c5c0286c3c6cc0bebf7832b04f476340114ea25e5c8216c4a1 "$native/src/filter_seccomp.c" | sha256sum --check --strict --quiet -
fi
cp "$native/COPYING" "$out/licenses/COPYING";cp "$native/LGPL-2.1-or-later" "$out/licenses/LGPL-2.1-or-later"
cp "$native/debian/copyright" "$out/licenses/debian.copyright";cp "$native/bundled/linux/COPYING" "$out/licenses/linux.COPYING"
cp "$native/debian/rules" "$out/inputs/debian.rules"
add_tree native-source "$native"
for query in cc1 collect2 as ld;do
  run "compiler-$query" 30 /usr/bin/gcc "-print-prog-name=$query"
  path=$(cat "$out/logs/compiler-$query.stdout");[[ $path == /* ]] || path=$(type -P "$path")
  bind_elf "$path"
done
mkdir "$build/native-build"
cflags=$(cat "$out/logs/flags-c.stdout");cppflags=$(cat "$out/logs/flags-cpp.stdout");ldflags=$(cat "$out/logs/flags-ld.stdout")
[[ -n $cflags && -n $cppflags && -n $ldflags && $cflags != *$'\n'* && $cppflags != *$'\n'* && $ldflags != *$'\n'* ]]
extra_env=(CC=/usr/bin/gcc CFLAGS="$cflags -Wall -g -O2" CPPFLAGS="$cppflags" LDFLAGS="$ldflags"
  CFLAGS_FOR_BUILD="$cflags -Wall -g -O2" CPPFLAGS_FOR_BUILD="$cppflags" LDFLAGS_FOR_BUILD="$ldflags")
cd "$build/native-build"
run configure 180 /bin/bash "$native/configure" --enable-mpers=check --disable-dependency-tracking --prefix="$build/install" --build=x86_64-linux-gnu
[[ -f config.status && ! -L config.status && $(stat -c %s config.status) -le 1048576 ]]
grep -Fx 'S["AMDEP_TRUE"]="#"' config.status > "$out/inputs/dependency-tracking.txt"
cp -- config.status "$out/inputs/config.status"
hold "$out/inputs/dependency-tracking.txt";hold "$out/inputs/config.status"
run make 300 /usr/bin/make -j2
cmp config.status "$out/inputs/config.status"
cd "$source";extra_env=()
cp "$build/native-build/config.log" "$out/inputs/config.log"
cp "$build/native-build/src/config.h" "$out/inputs/config.h"
tracer=$build/native-build/src/strace
[[ -x $tracer && ! -L $tracer && $(readlink -e "$tracer") == "$tracer" ]]
bind_elf "$tracer"
if [[ $mode == run-inherited ]];then inherited_controls;fi
[[ ! -e $cache && ! -L $cache ]] || fail 'cold cache was prepared before observation'
extra_env=(COLD560_LOG="$out/observer.log" OPENPILOT_CI_PREPARED_RUST_ROOT="$sdk" OPENPILOT_CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST")
if [[ $mode == run-inherited ]];then
  inherited_command cold 420 pass none "$tracer" --debug --seccomp-bpf -f -qq -s 65535 -e trace=execve,execveat "${launch_strip[@]}" \
    -o "$job/proof/prepared-cold-bootstrap.exec" "$source/rust/tools/check-disk-space" --path "$source" --growth-mib 2048
else
run cold 420 "$tracer" --debug --seccomp-bpf -f -qq -s 65535 -e trace=execve,execveat -E COLD560_LOG \
  -o "$job/proof/prepared-cold-bootstrap.exec" "$source/rust/tools/check-disk-space" --path "$source" --growth-mib 2048
fi
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
observer=true;hold "$out/observer.log";hold "$job/proof/prepared-cold-bootstrap.exec"
# cold 호출 뒤에만 현재 auditor를 build한다. 준비 단계에서 diskguard를 warm하지 않는다.
extra_env=(CARGO_TARGET_DIR="$build/audit-target")
run auditor-build 300 "$sdk/bin/cargo" test --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --no-default-features --test ci_disk_bootstrap --no-run --message-format=json
auditor=$(jq -ser --arg package "path+file://$source/rust/crates/dev-tools#openpilot-dev-tools@0.1.0" --arg manifest "$source/rust/crates/dev-tools/Cargo.toml" --arg src "$source/rust/crates/dev-tools/tests/ci_disk_bootstrap.rs" '
  if length<1 or .[-1]!={reason:"build-finished",success:true} or ([.[]|select(.reason=="build-finished")]|length)!=1 then error("Cargo EOF") else . end |
  [.[]|select(.reason=="compiler-artifact" and .package_id==$package and .manifest_path==$manifest and .target.name=="ci_disk_bootstrap" and .target.kind==["test"] and .target.src_path==$src and .profile.test==true and .profile.opt_level=="0" and .features==[] and .executable!=null)] |
  if length==1 then .[0].executable else error("exact current auditor ELF required") end' "$out/logs/auditor-build.stdout")
[[ $auditor == "$build/audit-target/"* && $(readlink -e "$auditor") == "$auditor" ]]
bind_elf "$auditor"
extra_env=(CI_PREPARED_RUST_PROOF="$job/proof")
run strict-audit 60 "$auditor" --ignored --exact prepared_audit::prepared_audit_actual_cold_bootstrap_receipt --nocapture --test-threads=1 --color=never
[[ $(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$out/logs/strict-audit.stdout") == 1 ]]
hold "$out/logs/strict-audit.stdout";hold "$job/proof/prepared-cold-bootstrap.json"
verify_marker_receipt "$out/logs/strict-audit.stdout" "$job/proof/prepared-cold-bootstrap.json" "$job/proof/prepared-cold-bootstrap.exec" "$out/strict-audit-marker.json"
hold "$out/strict-audit-marker.json"
audit=true;passed=true
