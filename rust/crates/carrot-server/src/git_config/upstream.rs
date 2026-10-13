use super::{
    commands::{git, Options},
    text, Failure, Repository,
};
use std::collections::HashMap;

pub(super) struct Selection<'a> {
    pub branch: &'a str,
    pub configured_remote: &'a str,
    pub remote: &'a str,
    pub heads: &'a HashMap<String, String>,
}

pub(super) struct Prepared {
    pub merge_refs: Vec<String>,
    pub target: String,
}

pub(super) fn choose(
    repository: &Repository<'_>,
    selection: &Selection<'_>,
    enabled: bool,
) -> Result<Prepared, Failure> {
    let mut target = String::new();
    let merge_key = format!("branch.{}.merge", selection.branch);
    let merge_refs = if selection.branch.is_empty() {
        Vec::new()
    } else {
        text::lines(&git(
            repository,
            &["config", "--get-all", &merge_key],
            Options {
                allowed: &[0, 1],
                ..Options::default()
            },
        )?)
    };
    if enabled && !selection.branch.is_empty() {
        if merge_refs.len() > 1 {
            let distinct = merge_refs
                .iter()
                .any(|value| Some(value) != merge_refs.first());
            if distinct && !selection.heads.contains_key(selection.branch) {
                return Err(Failure::Caught(
                    "Multiple upstream branches configured and no matching remote branch exists."
                        .into(),
                ));
            }
            let local = text::lines(&git(
                repository,
                &["config", "--local", "--get-all", &merge_key],
                Options {
                    allowed: &[0, 1],
                    ..Options::default()
                },
            )?);
            if merge_refs != local {
                return Err(Failure::Caught("Multiple upstream branches are inherited from outside this checkout; correct their source config.".into()));
            }
            if distinct {
                target = selection.branch.into();
            }
        }
        let tracked = merge_refs
            .first()
            .and_then(|value| value.strip_prefix("refs/heads/"))
            .unwrap_or("");
        if target.is_empty()
            && selection.configured_remote == selection.remote
            && selection.heads.contains_key(tracked)
        {
            target = tracked.into();
        } else if target.is_empty() && selection.heads.contains_key(selection.branch) {
            target = selection.branch.into();
        } else if target.is_empty() {
            return Err(Failure::Caught(format!("No valid upstream or matching remote branch for {}; select the intended branch before retrying.", selection.branch)));
        }
    }
    Ok(Prepared { merge_refs, target })
}

pub(super) fn finish(
    repository: &Repository<'_>,
    selection: &Selection<'_>,
    prepared: &Prepared,
) -> Result<String, Failure> {
    let upstream = format!("{}/{}", selection.remote, prepared.target);
    let fetched_head = git(
        repository,
        &["rev-parse", "--verify", &format!("refs/remotes/{upstream}")],
        Options::default(),
    )?;
    if selection.heads.get(&prepared.target) != Some(&fetched_head) {
        return Err(Failure::Caught(format!("Remote branch {upstream} changed or was excluded from fetch; retry after checking its fetch configuration.")));
    }
    let actual_upstream = git(
        repository,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
        Options {
            allowed: &[0, 128],
            ..Options::default()
        },
    )?;
    let expected_merge = format!("refs/heads/{}", prepared.target);
    if selection.configured_remote != selection.remote
        || prepared.merge_refs != [expected_merge.clone()]
        || actual_upstream != upstream
    {
        if prepared.merge_refs.len() > 1 {
            git(
                repository,
                &[
                    "config",
                    "--local",
                    "--replace-all",
                    &format!("branch.{}.merge", selection.branch),
                    &expected_merge,
                ],
                Options::default(),
            )?;
        }
        git(
            repository,
            &[
                "branch",
                &format!("--set-upstream-to={upstream}"),
                selection.branch,
            ],
            Options::default(),
        )?;
        Ok(format!(
            "Upstream repaired: {} -> {upstream}",
            selection.branch
        ))
    } else {
        Ok(format!(
            "Upstream verified: {} -> {upstream}",
            selection.branch
        ))
    }
}
