//! Native documentation gate. Preserves the policy in tools/docs/check_user_docs.py.
use serde::Deserialize;
use std::{
    collections::HashSet,
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
#[derive(Debug, Deserialize)]
pub struct Mapping {
    pub version: u32,
    pub rules: Vec<Rule>,
}
#[derive(Debug, Deserialize)]
pub struct Rule {
    pub id: String,
    pub description: String,
    pub code_paths: Vec<String>,
    pub doc_sets: Vec<Vec<String>>,
    #[serde(default = "required")]
    pub enforcement: String,
}
fn required() -> String {
    "required".into()
}

pub fn load_mapping(path: &Path) -> Result<Mapping> {
    let mapping: Mapping = serde_json::from_slice(&fs::read(path)?)?;
    if mapping.version != 1 {
        return Err("unsupported documentation map".into());
    }
    for rule in &mapping.rules {
        if rule.code_paths.is_empty()
            || rule.doc_sets.is_empty()
            || rule.doc_sets.iter().any(Vec::is_empty)
        {
            return Err(format!(
                "rule must contain code_paths and non-empty doc_sets: {}",
                rule.id
            )
            .into());
        }
        if !["required", "advisory"].contains(&rule.enforcement.as_str()) {
            return Err(format!("invalid enforcement: {}", rule.id).into());
        }
    }
    Ok(mapping)
}
// Python str.strip treats the four C0 information separators as whitespace in
// addition to the Unicode White_Space property used by Rust char::is_whitespace.
fn python_whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
pub fn normalize_path(path: &str) -> String {
    let path = path.trim_matches(python_whitespace).replace('\\', "/");
    path.strip_prefix("./").unwrap_or(&path).to_owned()
}

// fnmatchcase treats '/' and leading '.' as ordinary characters. A small token
// matcher avoids filesystem-glob differences and exponential wildcard backtracking.
#[derive(Debug)]
enum Token {
    Star,
    Any,
    Literal(char),
    Class(bool, Vec<(char, char)>),
}
fn tokens(pattern: &str) -> Vec<Token> {
    let p: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '*' => {
                if !matches!(out.last(), Some(Token::Star)) {
                    out.push(Token::Star);
                }
            }
            '?' => out.push(Token::Any),
            '[' => {
                let start = i + 1;
                let negated = p.get(start) == Some(&'!');
                let first = start + usize::from(negated);
                let mut end = first + usize::from(p.get(first) == Some(&']'));
                while end < p.len() && p[end] != ']' {
                    end += 1;
                }
                if end == p.len() {
                    out.push(Token::Literal('['));
                } else {
                    let mut ranges = Vec::new();
                    let mut j = first;
                    while j < end {
                        if j + 2 < end && p[j + 1] == '-' {
                            ranges.push((p[j], p[j + 2]));
                            j += 3;
                        } else {
                            ranges.push((p[j], p[j]));
                            j += 1;
                        }
                    }
                    out.push(Token::Class(negated, ranges));
                    i = end;
                }
            }
            c => out.push(Token::Literal(c)),
        }
        i += 1;
    }
    out
}
pub fn matches(path: &str, pattern: &str) -> bool {
    let text: Vec<char> = normalize_path(path).chars().collect();
    let mut previous = vec![false; text.len() + 1];
    previous[0] = true;
    for token in tokens(&normalize_path(pattern)) {
        let mut next = vec![false; previous.len()];
        if matches!(token, Token::Star) {
            next[0] = previous[0];
        }
        for (i, c) in text.iter().enumerate() {
            next[i + 1] = match &token {
                Token::Star => previous[i + 1] || next[i],
                Token::Any => previous[i],
                Token::Literal(expected) => previous[i] && c == expected,
                Token::Class(negated, ranges) => {
                    previous[i] && (ranges.iter().any(|(a, b)| a <= c && c <= b) != *negated)
                }
            };
        }
        previous = next;
    }
    previous[text.len()]
}
pub fn find_missing_docs<'a>(
    changed: &[String],
    mapping: &'a Mapping,
) -> Vec<(&'a Rule, Vec<String>)> {
    let changed: Vec<_> = changed
        .iter()
        .map(|p| normalize_path(p))
        .filter(|p| !p.is_empty())
        .collect();
    mapping
        .rules
        .iter()
        .filter_map(|rule| {
            let mut code: Vec<_> = changed
                .iter()
                .filter(|p| rule.code_paths.iter().any(|g| matches(p, g)))
                .cloned()
                .collect();
            code.sort();
            let complete = rule
                .doc_sets
                .iter()
                .any(|set| set.iter().all(|g| changed.iter().any(|p| matches(p, g))));
            (!code.is_empty() && !complete).then_some((rule, code))
        })
        .collect()
}
pub fn extract_override_reason(body: &str) -> &str {
    for line in body.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.contains('\r') {
            continue;
        }
        let line = line.trim_start_matches([' ', '\t']);
        if let Some((key, value)) = line.split_once(':') {
            if key
                .trim_end_matches([' ', '\t'])
                .eq_ignore_ascii_case("Docs-Not-Needed")
            {
                let reason = value.trim_matches(python_whitespace);
                let invalid = [
                    "n/a",
                    "na",
                    "none",
                    "없음",
                    "해당 없음",
                    "-",
                    "<reason>",
                    "<사유>",
                ];
                // For this fixed blocked-token set, long s is the only extra
                // Unicode casefold mapping beyond lowercase that can match.
                let folded = reason.to_lowercase().replace('\u{17f}', "s");
                return if reason.chars().count() >= 8 && !invalid.contains(&folded.as_str()) {
                    reason
                } else {
                    ""
                };
            }
        }
    }
    ""
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim_matches(python_whitespace)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?
        .trim_matches(python_whitespace)
        .into())
}
fn resolve_base(root: &Path, base: &str, head: &str) -> Result<String> {
    let base = base.trim_matches(python_whitespace);
    if !base.is_empty()
        && base != "0".repeat(40)
        && base != "0".repeat(64)
        && git(
            root,
            &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
        )
        .is_ok()
    {
        return Ok(base.into());
    }
    git(root, &["rev-parse", &format!("{head}^")])
}
#[derive(Debug)]
pub struct Options {
    pub root: PathBuf,
    pub map: PathBuf,
    pub base: String,
    pub head: String,
    pub commit_overrides: bool,
    pub changed: Vec<String>,
}
impl Options {
    pub fn new(root: PathBuf) -> Self {
        Self {
            map: root.join("docs/user/docs_map.json"),
            root,
            base: String::new(),
            head: "HEAD".into(),
            commit_overrides: false,
            changed: Vec::new(),
        }
    }
}
pub fn check(
    options: &Options,
    pr_body: &str,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<bool> {
    if options.commit_overrides && !options.changed.is_empty() {
        return Err("--commit-overrides requires a Git change range".into());
    }
    let mapping = load_mapping(&options.map)?;
    let base = if options.changed.is_empty() {
        Some(resolve_base(&options.root, &options.base, &options.head)?)
    } else {
        None
    };
    let changed = if let Some(base) = &base {
        git(
            &options.root,
            &[
                "diff",
                "--name-only",
                "--diff-filter=ACMRD",
                &format!("{base}...{}", options.head),
            ],
        )?
        .lines()
        .filter(|p| !p.trim_matches(python_whitespace).is_empty())
        .map(normalize_path)
        .collect()
    } else {
        options.changed.clone()
    };
    let mut missing = find_missing_docs(&changed, &mapping);
    let mut advisory = false;
    for (rule, paths) in &missing {
        if rule.enforcement == "advisory" {
            advisory = true;
            writeln!(
                out,
                "Docs review suggested [{}]: {}",
                rule.id,
                paths.join(", ")
            )?;
        }
    }
    if advisory {
        writeln!(out, "Update user guides only when documentation work is requested; keep web-only explanations in the localized UI.")?;
    }
    missing.retain(|(rule, _)| rule.enforcement == "required");
    if !missing.is_empty() && options.commit_overrides {
        let base = base.as_ref().ok_or("missing Git base")?;
        let commits = git(
            &options.root,
            &[
                "rev-list",
                "--first-parent",
                &format!("{base}..{}", options.head),
            ],
        )?;
        let mut unwaived = HashSet::new();
        for commit in commits.lines() {
            let message = git(&options.root, &["show", "-s", "--format=%B", commit])?;
            let reason = extract_override_reason(&message);
            if !reason.is_empty() {
                writeln!(out, "Commit {} docs exception: {reason}", &commit[..12])?;
            } else {
                unwaived.extend(
                    git(
                        &options.root,
                        &[
                            "diff-tree",
                            "--root",
                            "--first-parent",
                            "-m",
                            "--no-commit-id",
                            "--name-only",
                            "-r",
                            commit,
                        ],
                    )?
                    .lines()
                    .map(normalize_path),
                );
            }
        }
        for (_, paths) in &mut missing {
            paths.retain(|p| unwaived.contains(p));
        }
        missing.retain(|(_, paths)| !paths.is_empty());
    }
    if missing.is_empty() {
        writeln!(
            out,
            "User docs check passed ({} changed files).",
            changed.len()
        )?;
        return Ok(true);
    }
    let reason = extract_override_reason(pr_body);
    if !reason.is_empty() {
        writeln!(out, "User docs update skipped: {reason}")?;
        for (rule, paths) in missing {
            writeln!(out, "  - {}: {}", rule.id, paths.join(", "))?;
        }
        return Ok(true);
    }
    writeln!(err, "Required user-documentation update is missing:")?;
    for (rule, paths) in missing {
        writeln!(
            err,
            "\n[{}] {}\n  code: {}\n  update one complete set: {}",
            rule.id,
            rule.description,
            paths.join(", "),
            rule.doc_sets
                .iter()
                .map(|s| s.join(" + "))
                .collect::<Vec<_>>()
                .join("; or ")
        )?;
    }
    writeln!(err, "\nUpdate the relevant documents. For changes that do not affect settings behavior, add `Docs-Not-Needed: <concrete reason>` to the PR body or each affected push commit message.")?;
    Ok(false)
}
