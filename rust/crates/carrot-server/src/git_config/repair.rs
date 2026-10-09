use super::{
    commands::{git, Deadline, Options},
    text,
    upstream::{self, Selection},
    Failure, Repository,
};
use std::collections::HashMap;

pub(super) struct RepairOptions<'a> {
    pub remote: Option<&'a str>,
    pub upstream: bool,
}

pub(super) fn run(
    repository: &Repository<'_>,
    options: RepairOptions<'_>,
    messages: &mut Vec<String>,
) -> Result<Option<String>, Failure> {
    let branch = git(
        repository,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        Options {
            allowed: &[0, 1],
            ..Options::default()
        },
    )?;
    let configured_remote = if branch.is_empty() {
        String::new()
    } else {
        git(
            repository,
            &["config", "--get", &format!("branch.{branch}.remote")],
            Options {
                allowed: &[0, 1],
                ..Options::default()
            },
        )?
    };
    let remotes = text::lines(&git(repository, &["remote"], Options::default())?);
    let selected = options
        .remote
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if !configured_remote.is_empty() {
                &configured_remote
            } else if remotes.iter().any(|value| value == "origin") {
                "origin"
            } else {
                ""
            }
        });
    if selected == "." {
        return Ok(Some("Git configuration: local upstream preserved.".into()));
    }
    if selected.is_empty() && remotes.is_empty() {
        return Ok(Some("Git configuration: no remote configured.".into()));
    }
    if !remotes.iter().any(|value| value == selected) {
        return Err(Failure::Caught(
            "Current Git remote is missing; select the intended repository before retrying.".into(),
        ));
    }
    messages.push(format!("Checking Git configuration for {selected}."));
    let advertised = git(
        repository,
        &["ls-remote", "--heads", selected],
        Options {
            deadline: Deadline::Float(30),
            ..Options::default()
        },
    )?;
    let mut heads = HashMap::new();
    for line in text::lines(&advertised) {
        let parts: Vec<_> = line
            .split(text::whitespace)
            .filter(|value| !value.is_empty())
            .collect();
        if let [head, reference] = parts.as_slice() {
            if let Some(branch) = reference.strip_prefix("refs/heads/") {
                heads.insert(branch.to_owned(), (*head).to_owned());
            }
        }
    }
    let selection = Selection {
        branch: &branch,
        configured_remote: &configured_remote,
        remote: selected,
        heads: &heads,
    };
    let prepared = upstream::choose(repository, &selection, options.upstream)?;
    let key = format!("remote.{selected}.fetch");
    let specs = text::lines(&git(
        repository,
        &["config", "--get-all", &key],
        Options {
            allowed: &[0, 1],
            ..Options::default()
        },
    )?);
    let local_specs = text::lines(&git(
        repository,
        &["config", "--local", "--get-all", &key],
        Options {
            allowed: &[0, 1],
            ..Options::default()
        },
    )?);
    let mut obsolete = Vec::new();
    for spec in &specs {
        let source = spec
            .strip_prefix('+')
            .unwrap_or(spec)
            .split(':')
            .next()
            .unwrap_or("");
        if let Some(branch) = source.strip_prefix("refs/heads/") {
            if !source.contains('*') && !heads.contains_key(branch) {
                if !local_specs.contains(spec) {
                    return Err(Failure::Caught(format!("Obsolete fetch ref {source} is inherited from outside this checkout; its source config must be corrected.")));
                }
                if !obsolete.contains(spec) {
                    obsolete.push(spec.clone());
                }
            }
        }
    }
    for spec in obsolete {
        git(
            repository,
            &[
                "config",
                "--local",
                "--fixed-value",
                "--unset-all",
                &key,
                &spec,
            ],
            Options::default(),
        )?;
        messages.push(format!("Removed obsolete fetch ref: {spec}"));
    }
    let wildcard = format!("+refs/heads/*:refs/remotes/{selected}/*");
    if !specs.contains(&wildcard) {
        git(
            repository,
            &["config", "--local", "--add", &key, &wildcard],
            Options::default(),
        )?;
        messages.push(format!("Enabled remote branch tracking for {selected}."));
    }
    let mut fetch = vec!["fetch", "--prune", "--no-recurse-submodules", selected];
    let reference = format!(
        "+refs/heads/{}:refs/remotes/{selected}/{}",
        prepared.target, prepared.target
    );
    if !prepared.target.is_empty() {
        fetch.push(&reference);
    }
    let output = git(
        repository,
        &fetch,
        Options {
            deadline: Deadline::Float(180),
            ..Options::default()
        },
    )?;
    if !output.is_empty() {
        messages.push(output);
    }
    if !prepared.target.is_empty() {
        messages.push(upstream::finish(repository, &selection, &prepared)?);
    }
    messages.push("Git configuration verified.".into());
    Ok(None)
}
