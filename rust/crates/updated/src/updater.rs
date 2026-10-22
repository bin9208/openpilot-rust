use crate::{
    agnos::Agnos,
    params::Params,
    paths::Paths,
    process::{args, Commands},
    report, Error,
};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use openpilot_hardware_info::HardwareInfo;
use openpilot_logging::producer::Logger;
use openpilot_timed::clock::Clock;
use std::path::Path;

pub struct Context<'a> {
    pub paths: &'a Paths,
    pub commands: &'a mut dyn Commands,
    pub hardware: &'a dyn HardwareInfo,
    pub agnos: &'a mut dyn Agnos,
    pub clock: &'a dyn Clock,
    pub logger: &'a mut Logger,
    pub is_agnos: bool,
}
impl Context<'_> {
    pub fn now(&self) -> Result<DateTime<Utc>, Error> {
        let nanos = self.clock.wall_nanos()?;
        DateTime::from_timestamp(
            i64::try_from(nanos / 1_000_000_000)
                .map_err(|_| Error::Contract("wall clock range"))?,
            ((nanos % 1_000_000_000) / 1000 * 1000) as u32,
        )
        .ok_or(Error::Contract("wall clock range"))
    }
}
pub struct Updater<'a> {
    pub params: Params,
    pub context: Context<'a>,
    pub branches: IndexMap<String, Option<String>>,
    pub has_internet: bool,
    default_none: bool,
}
impl<'a> Updater<'a> {
    pub fn new(params: Params, context: Context<'a>) -> Self {
        Self {
            params,
            context,
            branches: IndexMap::new(),
            has_internet: false,
            default_none: false,
        }
    }
    pub fn target_branch(&mut self) -> Result<String, Error> {
        let branch = match self
            .params
            .text("UpdaterTargetBranch", self.context.logger)?
        {
            Some(branch) => branch,
            None => self.get_branch(&self.context.paths.base.clone())?,
        };
        Ok(
            if self.context.hardware.get_device_type()? == "tizi" && branch == "release3" {
                "release-tizi".into()
            } else {
                branch
            },
        )
    }
    fn branch_hash(&mut self, branch: &str) -> Option<String> {
        self.branches
            .entry(branch.into())
            .or_insert_with(|| {
                if self.default_none {
                    None
                } else {
                    Some(String::new())
                }
            })
            .clone()
    }
    pub fn get_branch(&mut self, path: &Path) -> Result<String, Error> {
        Ok(self
            .context
            .commands
            .run(
                &args(&["git", "rev-parse", "--abbrev-ref", "HEAD"]),
                Some(path),
            )?
            .trim_end()
            .into())
    }
    pub fn get_commit_hash(&mut self, path: &Path) -> Result<String, Error> {
        Ok(self
            .context
            .commands
            .run(&args(&["git", "rev-parse", "HEAD"]), Some(path))?
            .trim_end()
            .into())
    }
    pub fn update_ready(&mut self) -> Result<bool, Error> {
        let finalized = self.context.paths.finalized();
        if !crate::common::get_consistent_flag(&finalized) {
            return Ok(false);
        }
        if finalized.join(".runtime-update-ready.json").try_exists()? {
            return Ok(true);
        }
        let base = self.context.paths.base.clone();
        let current_hash = self.get_commit_hash(&base)?;
        let target = self.target_branch()?;
        let hash_mismatch = Some(current_hash) != self.branch_hash(&target);
        let current_branch = self.get_branch(&base)?;
        let target = self.target_branch()?;
        let branch_mismatch = current_branch != target;
        let staged_branch = self.get_branch(&finalized)?;
        let target = self.target_branch()?;
        Ok((hash_mismatch || branch_mismatch) && staged_branch == target)
    }
    pub fn update_available(&mut self) -> Result<bool, Error> {
        let merged = self.context.paths.merged();
        if !merged.is_dir() || self.branches.is_empty() {
            return Ok(false);
        }
        let current_hash = self.get_commit_hash(&merged)?;
        let target = self.target_branch()?;
        let hash_mismatch = Some(current_hash) != self.branch_hash(&target);
        let current_branch = self.get_branch(&merged)?;
        let target = self.target_branch()?;
        Ok(hash_mismatch || current_branch != target)
    }
    pub fn setup_git_options(&mut self, path: &Path) -> Result<(), Error> {
        for (key, value) in [
            ("core.trustctime", "false"),
            ("core.checkStat", "minimal"),
            ("protocol.version", "2"),
            ("gc.auto", "0"),
            ("gc.autoDetach", "false"),
        ] {
            self.context
                .commands
                .run(&args(&["git", "config", key, value]), Some(path))?;
        }
        Ok(())
    }
    fn update_remote(&mut self, path: &Path) -> Result<String, Error> {
        let branch = self.get_branch(path)?;
        let remote = match self.context.commands.run(
            &args(&["git", "config", "--get", &format!("branch.{branch}.remote")]),
            Some(path),
        ) {
            Ok(value) => value.strip_suffix('\n').unwrap_or(&value).to_owned(),
            // Only an absent branch setting permits the conventional fallback.
            Err(Error::Command { code: 1, .. }) => "origin".into(),
            Err(error) => return Err(error),
        };
        if remote.is_empty()
            || remote == "."
            || remote.starts_with('-')
            || !remote
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_./-".contains(&c))
        {
            return Err(Error::Contract("unsupported update remote name"));
        }
        // get-url requires a configured remote and expands insteadOf mappings.
        // Validate that effective fetch URL, rather than trusting a named alias.
        let url = self
            .context
            .commands
            .run(&args(&["git", "remote", "get-url", &remote]), Some(path))?;
        if !supported_remote_url(url.strip_suffix('\n').unwrap_or(&url)) {
            return Err(Error::Contract(
                "update remote must use a supported network URL",
            ));
        }
        Ok(remote)
    }
    pub fn check_for_update(&mut self) -> Result<(), Error> {
        report::info(self.context.logger, "checking for updates")?;
        let merged = self.context.paths.merged();
        let remote = self.update_remote(&merged)?;
        match self
            .context
            .commands
            .run(&args(&["git", "ls-remote", &remote, "HEAD"]), Some(&merged))
        {
            Ok(_) => self.has_internet = true,
            Err(Error::Command { .. }) => self.has_internet = false,
            Err(error) => return Err(error),
        }
        self.setup_git_options(&merged)?;
        let output = self.context.commands.run(
            &args(&["git", "ls-remote", "--heads", &remote]),
            Some(&merged),
        )?;
        self.branches.clear();
        self.default_none = true;
        for line in output.split('\n') {
            if let Some((hash, branch)) = remote_head(line) {
                self.branches.insert(branch.into(), Some(hash.into()));
            }
        }
        let current_branch = self.get_branch(&merged)?;
        let current_hash = self.get_commit_hash(&merged)?;
        let target = self.target_branch()?;
        let hash = self.branch_hash(&target);
        if current_branch != target || Some(&current_hash) != hash.as_ref() {
            report::info(
                self.context.logger,
                &format!(
                    "update available, {current_branch} ({}) -> {target} ({})",
                    short(&current_hash),
                    short(hash.as_deref().unwrap_or("None"))
                ),
            )?;
        } else {
            report::info(
                self.context.logger,
                &format!("up to date on {current_branch} ({})", short(&current_hash)),
            )?;
        }
        Ok(())
    }
    pub fn fetch_update(&mut self) -> Result<(), Error> {
        let merged = self.context.paths.merged();
        let remote = self.update_remote(&merged)?;
        report::info(
            self.context.logger,
            "attempting git fetch inside staging overlay",
        )?;
        self.params.put("UpdaterState", b"downloading...")?;
        if !self
            .context
            .paths
            .finalized()
            .join(".runtime-update-ready.json")
            .try_exists()?
        {
            crate::common::set_consistent_flag(
                self.context.paths,
                &self.context.paths.finalized(),
                false,
            )?;
            self.params.put_bool("UpdateAvailable", false)?;
        }
        self.setup_git_options(&merged)?;
        self.context.commands.run(
            &args(&[
                "git",
                "config",
                "--replace-all",
                &format!("remote.{remote}.fetch"),
                &format!("+refs/heads/*:refs/remotes/{remote}/*"),
            ]),
            Some(&merged),
        )?;
        let branch = self.target_branch()?;
        let output = self
            .context
            .commands
            .run(&args(&["git", "fetch", &remote, &branch]), Some(&merged))?;
        report::info(self.context.logger, &format!("git fetch success: {output}"))?;
        report::info(self.context.logger, "git reset in progress")?;
        let commands = [
            args(&[
                "git",
                "checkout",
                "--force",
                "--no-recurse-submodules",
                "-B",
                &branch,
                "FETCH_HEAD",
            ]),
            args(&[
                "git",
                "branch",
                "--set-upstream-to",
                &format!("{remote}/{branch}"),
            ]),
            args(&["git", "reset", "--hard"]),
            args(&["git", "clean", "-xdff"]),
            args(&["git", "submodule", "sync"]),
            args(&["git", "submodule", "update", "--init", "--recursive"]),
            args(&[
                "git",
                "submodule",
                "foreach",
                "--recursive",
                "git",
                "reset",
                "--hard",
            ]),
        ];
        let mut outputs = Vec::new();
        for command in commands {
            outputs.push(self.context.commands.run(&command, Some(&merged))?);
        }
        report::info(
            self.context.logger,
            &format!("git reset success: {}", outputs.join("\n")),
        )?;
        if self.context.is_agnos {
            self.handle_agnos_update()?;
        }
        self.params.put("UpdaterState", b"finalizing update...")?;
        self.finalize_update()?;
        report::info(self.context.logger, "finalize success!")?;
        Ok(())
    }
}
fn supported_remote_url(value: &str) -> bool {
    if value
        .bytes()
        .any(|c| c.is_ascii_control() || c.is_ascii_whitespace())
    {
        return false;
    }
    if value.contains("://") {
        return url::Url::parse(value).is_ok_and(|url| {
            matches!(url.scheme(), "https" | "http" | "ssh" | "git") && url.host_str().is_some()
        });
    }
    // Git's scp-style SSH syntax: [user@]host:path. A local path or remote
    // helper (such as ext::command) must not be interpreted as this form.
    let host_path = if let Some((user, host_path)) = value.split_once('@') {
        if user.is_empty()
            || user.starts_with('-')
            || !user
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return false;
        }
        host_path
    } else {
        value
    };
    let path = if let Some(address_path) = host_path.strip_prefix('[') {
        let Some((address, path)) = address_path.split_once("]:") else {
            return false;
        };
        if address.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        path
    } else {
        let Some((host, path)) = host_path.split_once(':') else {
            return false;
        };
        if host.is_empty()
            || matches!(host, "." | "..")
            || host.starts_with('-')
            || !host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return false;
        }
        path
    };
    !path.is_empty() && !path.starts_with(':')
}
fn short(value: &str) -> String {
    value.chars().take(7).collect()
}
pub fn remote_head(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\x1c'..='\x1f'));
    let (hash, rest) =
        line.split_once(|c: char| c.is_whitespace() || matches!(c, '\x1c'..='\x1f'))?;
    if !(5..=40).contains(&hash.len())
        || !hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return None;
    }
    let branch = rest
        .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '\x1c'..='\x1f'))
        .strip_prefix("refs/heads/")?;
    if matches!(branch, "release2" | "release2-staging") {
        return None;
    }
    Some((hash, branch))
}

#[cfg(test)]
mod tests {
    use super::supported_remote_url;

    #[test]
    fn network_urls_accept_repository_roots_and_bracketed_ipv6() {
        for url in [
            "https://git.example/",
            "http://git.example/",
            "ssh://git@git.example/",
            "git://git.example/",
            "git@[2001:db8::1]:repo.git",
            "[2001:db8::1]:repo.git",
            "git@[::1]:/srv/git/repo.git",
            "ssh://git@[2001:db8::1]/repo.git",
        ] {
            assert!(supported_remote_url(url), "rejected network URL: {url}");
        }
    }

    #[test]
    fn malformed_ipv6_and_unsupported_or_local_sources_stay_rejected() {
        for url in [
            "git@[2001:db8::zz]:repo.git",
            "git@[2001:db8:::1]:repo.git",
            "git@[127.0.0.1]:repo.git",
            "git@[]:repo.git",
            "git@[2001:db8::1:repo.git",
            "git@[2001:db8::1]extra:repo.git",
            "git@[2001:db8::1]:",
            "git@[2001:db8::1]::helper",
            "-user@[2001:db8::1]:repo.git",
            "git@@[2001:db8::1]:repo.git",
            "@[2001:db8::1]:repo.git",
            "ext::false",
            "ftp://git.example/",
            "file:///repo.git",
            "/repo.git",
            "../repo.git",
            ".:repo.git",
        ] {
            assert!(!supported_remote_url(url), "accepted invalid source: {url}");
        }
    }
}
