#!/usr/bin/env bash
# ssh 원본1의 sampled absence 전용 함수다. source하여 쓰며 원본/Python/후보 ELF는 실행하지 않는다.
# 명령 경계/2초 sampling은 whole Python absence나 연속 filesystem 감시가 아니다.

_ssh_absence_error() {
  printf '%s\n' "$2" >&2
  return "$1"
}

_ssh_absence_parents() {
  local root=$1 path canonical uid metadata seal=
  [[ $root == /* && $root != *$'\n'* && $root != *$'\t'* ]] || {
    _ssh_absence_error 43 'ssh source absence inspection failed'; return $?;
  }
  canonical=$(readlink -e -- "$root") || {
    _ssh_absence_error 43 'ssh source absence inspection failed'; return $?;
  }
  [[ $canonical == "$root" ]] || {
    _ssh_absence_error 42 'ssh source parent boundary differs'; return $?;
  }
  uid=$(id -u) || return 43
  for path in "$root" "$root/tools" "$root/tools/scripts"; do
    [[ -d $path && ! -L $path && -r $path && -x $path ]] || {
      _ssh_absence_error 42 'ssh source parent boundary differs'; return $?;
    }
    metadata=$(stat -c '%d:%i:%f:%u:%g' -- "$path") || return 43
    [[ $metadata =~ ^[0-9]+:[0-9]+:[a-f0-9]+:${uid}:[0-9]+$ ]] || {
      _ssh_absence_error 42 'ssh source parent boundary differs'; return $?;
    }
    seal+="${seal:+|}$metadata"
  done
  printf '%s\n' "$seal"
}

_ssh_absence_check() {
  local root=$1 expected=$2 observed entries
  observed=$(_ssh_absence_parents "$root") || return $?
  [[ $observed == "$expected" ]] || {
    _ssh_absence_error 42 'ssh source parent boundary differs'; return $?;
  }
  # find는 dangling symlink도 entry로 세며, 조사 오류를 missing으로 바꾸지 않는다.
  entries=$(find -P "$root/tools/scripts" -mindepth 1 -maxdepth 1 -name ssh.py -printf x) || {
    _ssh_absence_error 43 'ssh source absence inspection failed'; return $?;
  }
  [[ -z $entries ]] || {
    _ssh_absence_error 41 'ssh source entry reappeared'; return $?;
  }
  observed=$(_ssh_absence_parents "$root") || return $?
  [[ $observed == "$expected" ]] || {
    _ssh_absence_error 42 'ssh source parent boundary differs'; return $?;
  }
}

_ssh_absence_proof() {
  local job=${CI_JOB_TEMP:-} proof=${CI_JOB_TEMP:-}/proof
  [[ $job == /* && -d $job && ! -L $job && $(readlink -e "$job") == "$job" &&
     $(stat -c '%u:%a' "$job") == "$(id -u):700" &&
     -f $job/owner && ! -L $job/owner &&
     $(cat "$job/owner") == "${GITHUB_RUN_ID:-}:${GITHUB_RUN_ATTEMPT:-}:${GITHUB_JOB:-}" &&
     -d $proof && ! -L $proof && $(readlink -e "$proof") == "$proof" &&
     $(stat -c %u "$proof") == "$(id -u)" ]] || return 43
}

_ssh_absence_file() {
  [[ -f $1 && ! -L $1 && $(stat -c '%u:%a:%h' "$1") == "$(id -u):600:1" &&
     $(stat -c %s "$1") -le 1048576 ]] || return 43
}

# upload 뒤 cleanup 직전에도 사용하는 순수 검사다. journal/sticky 파일을 쓰지 않는다.
ssh_absence_inspect() {
  local root=$1 source_sha=$2
  _ssh_absence_proof || return $?
  [[ ${SSH_SOURCE_PHASE:-} == absent && $root == "${DIAG_SOURCE:-}" &&
     $root == "${SSH_ABSENCE_ROOT:-}" && $source_sha == "${GITHUB_SHA:-}" &&
     $source_sha == "${SSH_ABSENCE_SOURCE_SHA:-}" && $source_sha =~ ^[a-f0-9]{40}$ &&
     -n ${SSH_ABSENCE_PARENTS:-} ]] || {
    _ssh_absence_error 43 'ssh source absence context differs'; return $?;
  }
  [[ ! -e $CI_JOB_TEMP/proof/ssh-absence-failure && ! -L $CI_JOB_TEMP/proof/ssh-absence-failure ]] || {
    _ssh_absence_error 43 'ssh source absence earlier failure'; return $?;
  }
  _ssh_absence_check "$root" "$SSH_ABSENCE_PARENTS"
}

ssh_absence_verify() {
  local root=$1 source_sha=$2 stage=$3 code=0 proof=${CI_JOB_TEMP:-}/proof
  [[ $stage =~ ^[a-z0-9_-]+$ ]] || return 43
  ssh_absence_inspect "$root" "$source_sha" || code=$?
  if (( code != 0 )); then
    # 최초 실패만 남긴다. 잘못된/외부 proof에는 쓰지 않는다.
    if _ssh_absence_proof; then
      (umask 077; set -o noclobber; printf '%s\t%s\t%s\n' "$stage" "$code" "$EPOCHREALTIME" > "$proof/ssh-absence-failure") 2>/dev/null || :
    fi
    return "$code"
  fi
  _ssh_absence_file "$proof/ssh-absence-checks.tsv" || return $?
  printf '%s\t%s\t%s\ttools/scripts/ssh.py\t%s\t%s\tabsent\n' \
    "$stage" "$source_sha" "$root" "$SSH_ABSENCE_PARENTS" "$EPOCHREALTIME" >> "$proof/ssh-absence-checks.tsv"
}

ssh_absence_admit() {
  local root=$1 source_sha=$2 proof=$3 owner=${DIAG_SOURCE_OWNER:-}
  _ssh_absence_proof || return $?
  [[ ${SSH_SOURCE_PHASE:-} == absent && $root == "${DIAG_SOURCE:-}" &&
     $source_sha == "${GITHUB_SHA:-}" && $source_sha =~ ^[a-f0-9]{40}$ &&
     $proof == "$CI_JOB_TEMP/proof" && $root == "$owner/source" &&
     -d $owner && ! -L $owner && $(readlink -e "$owner") == "$owner" &&
     $(stat -c %u "$owner") == "$(id -u)" &&
     $(stat -c '%d:%i' "$owner") == "${DIAG_SOURCE_IDENTITY:-}" &&
     -f $owner/.diag296-owner && ! -L $owner/.diag296-owner &&
     $(cat "$owner/.diag296-owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" ]] || return 43
  SSH_ABSENCE_ROOT=$root
  SSH_ABSENCE_SOURCE_SHA=$source_sha
  SSH_ABSENCE_PARENTS=$(_ssh_absence_parents "$root") || return $?
  export SSH_ABSENCE_ROOT SSH_ABSENCE_SOURCE_SHA SSH_ABSENCE_PARENTS
  _ssh_absence_check "$root" "$SSH_ABSENCE_PARENTS" || return $?
  [[ $(git --no-optional-locks -C "$root" rev-parse HEAD) == "$source_sha" ]] || return 43
  (umask 077; set -o noclobber
    git --no-optional-locks -C "$root" ls-tree -z HEAD -- tools/scripts/ssh.py > "$proof/ssh-absence-head.bin" &&
    git --no-optional-locks -C "$root" ls-files -z -- tools/scripts/ssh.py > "$proof/ssh-absence-index.bin" &&
    printf '%s\t%s\ttools/scripts/ssh.py\tf4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff\t%s\n' \
      "$source_sha" "$root" "$SSH_ABSENCE_PARENTS" > "$proof/ssh-absence-before.tsv" &&
    : > "$proof/ssh-absence-checks.tsv") || return 43
  [[ ! -s $proof/ssh-absence-head.bin && ! -s $proof/ssh-absence-index.bin ]] || {
    _ssh_absence_error 43 'ssh source remains in Git roster'; return $?;
  }
  ssh_absence_verify "$root" "$source_sha" checkout
}

ssh_absence_require_stages() {
  local proof=$1 stage
  [[ $proof == "${CI_JOB_TEMP:-}/proof" ]] || return 43
  ssh_absence_inspect "$SSH_ABSENCE_ROOT" "$SSH_ABSENCE_SOURCE_SHA" || return $?
  for stage in before.tsv head.bin index.bin checks.tsv controls.tsv; do
    _ssh_absence_file "$proof/ssh-absence-$stage" || return $?
  done
  printf '%s\t%s\ttools/scripts/ssh.py\tf4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff\t%s\n' \
    "$SSH_ABSENCE_SOURCE_SHA" "$SSH_ABSENCE_ROOT" "$SSH_ABSENCE_PARENTS" |
    cmp - "$proof/ssh-absence-before.tsv" || return 43
  [[ ! -s $proof/ssh-absence-head.bin && ! -s $proof/ssh-absence-index.bin ]] || return 43
  awk -F '\t' -v sha="$SSH_ABSENCE_SOURCE_SHA" -v root="$SSH_ABSENCE_ROOT" -v parents="$SSH_ABSENCE_PARENTS" '
    NF != 7 || $2 != sha || $3 != root || $4 != "tools/scripts/ssh.py" || $5 != parents ||
      $6 !~ /^[0-9]+[.][0-9]+$/ || $7 != "absent" {bad=1}
    {seen[$1]++}
    END {
      split("checkout controls-before controls-after historical-deletion frozen-before frozen-after clippy-before clippy-after inputs-before inputs-after source-final-before source-final-after sdk-final-after driver-entry driver-before-seals driver-before-receipt driver-before-publish closure", stages, " ");
      for (i in stages) if (seen[stages[i]] != 1) bad=1;
      split("registry-copy public-build entry-build recorder-build strace-help entry-0 entry-1 entry-2 entry-3", commands, " ");
      for (i in commands) if (seen["command-before-" commands[i]] != 1 || seen["command-after-" commands[i]] != 1) bad=1;
      if (seen["sampled-budget"] < 1) bad=1;
      for (stage in seen) {
        known=(stage=="sampled-budget");
        for (i in stages) if (stage==stages[i]) known=1;
        for (i in commands) if (stage=="command-before-" commands[i] || stage=="command-after-" commands[i]) known=1;
        if (!known) bad=1;
      }
      exit bad;
    }' "$proof/ssh-absence-checks.tsv" || return 43
  [[ $(cat "$proof/ssh-absence-controls.tsv") == $'absent\tpass\nregular-reappearance\tpass\ndirectory\tpass\nsymlink\tpass\ndangling-symlink\tpass\nparent-symlink\tpass' ]] || return 43
}

ssh_absence_controls() {
  local job=$1 proof=$2 control expected label code observed
  [[ $job == "${CI_JOB_TEMP:-}" && $proof == "$job/proof" ]] || return 43
  _ssh_absence_proof || return $?
  control=$(mktemp -d "$job/ssh-absence-controls.XXXXXX") || return 43
  chmod 700 "$control" || return 43
  mkdir "$control/tools" "$control/tools/scripts" || return 43
  printf 'inert control\n' > "$control/target" || return 43
  expected=$(_ssh_absence_parents "$control") || return $?
  (umask 077; set -o noclobber; : > "$proof/ssh-absence-controls.tsv") || return 43
  _ssh_absence_check "$control" "$expected" || return $?
  printf 'absent\tpass\n' >> "$proof/ssh-absence-controls.tsv" || return 43
  for label in regular-reappearance directory symlink dangling-symlink parent-symlink; do
    case "$label" in
      regular-reappearance) printf 'inert control\n' > "$control/tools/scripts/ssh.py" || return 43 ;;
      directory) mkdir "$control/tools/scripts/ssh.py" || return 43 ;;
      symlink) ln -s "$control/target" "$control/tools/scripts/ssh.py" || return 43 ;;
      dangling-symlink) ln -s "$control/missing" "$control/tools/scripts/ssh.py" || return 43 ;;
      parent-symlink) mv "$control/tools/scripts" "$control/saved-scripts" && ln -s "$control/saved-scripts" "$control/tools/scripts" || return 43 ;;
    esac
    code=0
    observed=$(_ssh_absence_check "$control" "$expected" 2>&1) || code=$?
    if [[ $label == parent-symlink ]]; then
      [[ $code == 42 && $observed == 'ssh source parent boundary differs' ]] || return 43
      rm -- "$control/tools/scripts" && mv "$control/saved-scripts" "$control/tools/scripts" || return 43
    else
      [[ $code == 41 && $observed == 'ssh source entry reappeared' ]] || return 43
      if [[ $label == directory ]]; then rm -d -- "$control/tools/scripts/ssh.py" || return 43
      else rm -- "$control/tools/scripts/ssh.py" || return 43; fi
    fi
    printf '%s\tpass\n' "$label" >> "$proof/ssh-absence-controls.tsv" || return 43
    _ssh_absence_check "$control" "$expected" || return $?
  done
}
