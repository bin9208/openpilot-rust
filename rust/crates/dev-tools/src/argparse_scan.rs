//! Opt-in scanner for the argparse grammar used by native build tools.
use std::collections::BTreeMap;
#[derive(Debug)]
pub struct ArgumentError(pub String);
impl std::fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ArgumentError {}
#[derive(Clone, Copy)]
pub struct OptionSpec {
    pub name: &'static str,
    takes_value: bool,
    required: bool,
    group: Option<&'static str>,
}
impl OptionSpec {
    pub const fn value(name: &'static str, required: bool) -> Self {
        Self {
            name,
            takes_value: true,
            required,
            group: None,
        }
    }
    pub const fn flag(name: &'static str, group: Option<&'static str>) -> Self {
        Self {
            name,
            takes_value: false,
            required: false,
            group,
        }
    }
}
pub struct Parsed {
    pub values: BTreeMap<String, String>,
    pub help: bool,
    pub positionals: Vec<String>,
}
fn option(value: &str) -> bool {
    static NEGATIVE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    // argparse treats unknown strings containing ASCII spaces as positional values.
    !value.contains(' ')
        && value != "-"
        && value.starts_with('-')
        && !NEGATIVE
            .get_or_init(|| {
                regex::Regex::new(r"^-\d+$|^-\d*\.\d+$").expect("fixed negative-number pattern")
            })
            // Python's `$` also matches immediately before one final newline.
            .is_match(value.strip_suffix('\n').unwrap_or(value))
}
pub fn scan(args: &[String], specs: &[OptionSpec]) -> Result<Parsed, ArgumentError> {
    scan_with_positionals(args, specs, 0)
}
/// Opt in to a fixed number of required positional arguments, including `--` literals.
pub fn scan_with_positionals(
    args: &[String],
    specs: &[OptionSpec],
    required_positionals: usize,
) -> Result<Parsed, ArgumentError> {
    let mut all = specs.to_vec();
    all.push(OptionSpec::flag("--help", None));
    let resolve = |name: &str| -> Result<Option<usize>, ArgumentError> {
        // A short-option cluster executes help before consuming its remaining letters.
        if name.starts_with("-h") && !name.starts_with("--") {
            return Ok(Some(all.len() - 1));
        }
        if let Some(i) = all.iter().position(|s| s.name == name) {
            return Ok(Some(i));
        }
        let candidates = all
            .iter()
            .enumerate()
            .filter(|(_, s)| name.starts_with("--") && s.name.starts_with(name))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Ok(None),
            [i] => Ok(Some(*i)),
            _ => Err(ArgumentError(format!("ambiguous option: {name}"))),
        }
    };
    // argparse classifies every option (and rejects ambiguity) before executing help/actions.
    let mut classified = Vec::new();
    let mut terminated = false;
    for arg in args {
        if arg == "--" {
            terminated = true;
            classified.push(None);
            continue;
        }
        classified.push(if terminated {
            None
        } else {
            resolve(arg.split_once('=').map_or(arg.as_str(), |(n, _)| n))?
        });
    }
    let mut values = BTreeMap::new();
    let mut groups = BTreeMap::new();
    let mut extras = false;
    let mut positionals = Vec::new();
    let mut literal = false;
    let mut i = 0;
    while i < args.len() {
        let Some(index) = classified[i] else {
            if required_positionals > 0 && args[i] == "--" && !literal {
                literal = true;
            } else if required_positionals > 0 && (literal || !option(&args[i])) {
                positionals.push(args[i].clone());
            } else {
                extras = true;
            }
            i += 1;
            continue;
        };
        let spec = all[index];
        let inline =
            if args[i].starts_with("-h") && !args[i].starts_with("-h=") && args[i].len() > 2 {
                None
            } else {
                args[i].split_once('=').map(|(_, v)| v)
            };
        if !spec.takes_value {
            if inline.is_some() {
                return Err(ArgumentError(format!(
                    "{} does not accept a value",
                    spec.name
                )));
            }
            if spec.name == "--help" {
                return Ok(Parsed {
                    values,
                    help: true,
                    positionals,
                });
            }
            if let Some(group) = spec.group {
                if let Some(previous) = groups.insert(group, spec.name) {
                    if previous != spec.name {
                        return Err(ArgumentError(format!(
                            "{} conflicts with {previous}",
                            spec.name
                        )));
                    }
                }
            }
            values.insert(spec.name.to_owned(), String::new());
        } else {
            let value = if let Some(v) = inline {
                v
            } else {
                i += 1;
                args.get(i)
                    .filter(|v| classified[i].is_none() && !option(v))
                    .ok_or_else(|| ArgumentError(format!("{} requires a value", spec.name)))?
            };
            values.insert(spec.name.to_owned(), value.to_owned());
        }
        i += 1;
    }
    if extras || positionals.len() != required_positionals {
        return Err(ArgumentError("unrecognized arguments".into()));
    }
    for spec in specs {
        if spec.required && !values.contains_key(spec.name) {
            return Err(ArgumentError(format!("{} required", spec.name)));
        }
    }
    Ok(Parsed {
        values,
        help: false,
        positionals,
    })
}
