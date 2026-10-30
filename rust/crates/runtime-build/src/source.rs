use crate::Result;
use serde::Serialize;
use std::{path::Path, process::Command};

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Source {
    pub commit: String,
    pub tree: String,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()?;
    if !output.status.success() {
        return Err("cannot inspect runtime build source checkout".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim_end().to_owned())
}

pub fn inspect(root: &Path) -> Result<Source> {
    if Path::new(&git(root, &["rev-parse", "--show-toplevel"])?) != root {
        return Err("runtime build root must be the source checkout root".into());
    }
    if !git(
        root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?
    .is_empty()
    {
        return Err(
            "runtime build source must be clean, including submodules and untracked inputs".into(),
        );
    }
    let commit = git(root, &["rev-parse", "--verify", "HEAD"])?;
    let tree = git(root, &["rev-parse", "--verify", "HEAD^{tree}"])?;
    if [commit.as_str(), tree.as_str()].iter().any(|value| {
        value.len() != 40
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        return Err("runtime build source requires full Git SHA-1 identities".into());
    }
    Ok(Source { commit, tree })
}
