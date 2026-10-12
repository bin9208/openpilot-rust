use super::{git_config::text, process};
use crate::{config::Config, Error, Value};

pub(super) fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&c))
    {
        return text.into();
    }
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}
fn parameter(payload: &Value, key: &str) -> Result<String, Error> {
    let value = payload.get(key);
    if !value.truth() {
        return Ok(String::new());
    }
    Ok(value.string()?.trim_matches(text::whitespace).into())
}
pub(super) fn git(config: &Config, action: &str, payload: &Value) -> Result<Option<String>, Error> {
    let command = match action {
        "git_pull" => "__git-pull".into(),
        "git_sync" => {
            let current = process::run(config, &["git", "branch", "--show-current"], 15, None)?.combined();
            let current = current.trim_matches(text::whitespace);
            let branches = process::run(config, &["git", "branch", "--format=%(refname:short)"], 30, None)?;
            let mut parts = Vec::new();
            if branches.code == 0 {
                for branch in text::lines(&branches.combined()) {
                    let branch = branch.trim_matches(text::whitespace);
                    if !branch.is_empty() && branch != current { parts.push(format!("git branch -D {}", quote(branch))); }
                }
            }
            parts.push("git fetch --all --prune".into());
            parts.join(" && ")
        }
        "git_reset" => "git reset --hard HEAD".into(),
        "git_checkout" => {
            let branch = parameter(payload, "branch")?;
            if branch.is_empty() { return Ok(None); }
            match branch.strip_prefix("origin/") {
                Some(local) => format!("git fetch --all --prune && git switch -C {} --track {}", quote(local), quote(&branch)),
                None => format!("git fetch --all --prune && git switch {}", quote(&branch)),
            }
        }
        "git_checkout_commit" => {
            let commit = parameter(payload, "commit")?;
            if commit.is_empty() { return Ok(None); }
            format!("git checkout {}", quote(&commit))
        }
        "git_reset_repo" => {
            let branch = parameter(payload, "branch")?;
            if branch.is_empty() { return Ok(None); }
            let branch = quote(&branch);
            format!("git remote set-url origin https://github.com/ajouatom/openpilot.git 2>/dev/null; git merge --abort 2>/dev/null; git rebase --abort 2>/dev/null; git cherry-pick --abort 2>/dev/null; git revert --abort 2>/dev/null; git am --abort 2>/dev/null; git bisect reset 2>/dev/null; git pack-refs --all 2>/dev/null; rm -rf .git/refs/remotes/origin 2>/dev/null; git fetch origin --prune --force && git checkout -f -B {branch} origin/{branch} && git reset --hard origin/{branch} && git clean -xfd")
        }
        "git_rebuild" => "cd /data/openpilot && scons -c && rm -f .sconsign.dblite && rm -rf /tmp/scons_cache && rm -f prebuilt".into(),
        _ => return Ok(None),
    };
    let executable = std::env::current_exe()?;
    let wrapper = format!(
        "{} __repo-update --repo {}",
        quote(&executable.to_string_lossy()),
        quote(&config.repository.to_string_lossy())
    );
    Ok(Some(if action == "git_pull" {
        format!("{wrapper} {command}")
    } else {
        format!("{wrapper} bash -c {}", quote(&command))
    }))
}
