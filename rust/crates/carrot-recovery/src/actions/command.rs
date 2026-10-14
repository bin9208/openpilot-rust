use super::git_config::text;
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
        "git_sync" => "__fetch-native".into(),
        "git_reset" | "git_rebuild" => "__stage-repair".into(),
        "git_checkout" | "git_reset_repo" => {
            let branch = parameter(payload, "branch")?;
            if branch.is_empty() {
                return Ok(None);
            }
            format!(
                "{} {}",
                if action == "git_checkout" {
                    "__stage-checkout"
                } else {
                    "__stage-reset-repo"
                },
                quote(&branch)
            )
        }
        "git_checkout_commit" => {
            let commit = parameter(payload, "commit")?;
            if commit.is_empty() {
                return Ok(None);
            }
            format!("__stage-commit {}", quote(&commit))
        }
        _ => return Ok(None),
    };
    let executable = std::env::current_exe()?;
    let wrapper = format!(
        "{} __repo-update --repo {}",
        quote(&executable.to_string_lossy()),
        quote(&config.repository.to_string_lossy())
    );
    Ok(Some(format!("{wrapper} {command}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maintenance_commands_request_native_staging_without_checkout_mutation() {
        let config = Config::original().unwrap();
        for (action, expected) in [
            ("git_reset", "__stage-repair"),
            ("git_rebuild", "__stage-repair"),
            ("git_checkout", "__stage-checkout"),
            ("git_checkout_commit", "__stage-commit"),
            ("git_reset_repo", "__stage-reset-repo"),
        ] {
            let payload = Value::object([
                ("branch", Value::text("candidate")),
                ("commit", Value::text("1234567890abcdef")),
            ]);
            let command = git(&config, action, &payload).unwrap().unwrap();
            assert!(
                command.contains(expected),
                "{action} bypassed native staging: {command}"
            );
            for unsafe_command in [
                "git clean",
                "git reset --hard",
                "git switch",
                "scons",
                "ajouatom",
            ] {
                assert!(!command.contains(unsafe_command));
            }
        }
    }
}
