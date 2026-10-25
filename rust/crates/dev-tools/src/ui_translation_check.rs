//! Independent reference loader for the original UI PO checker; no candidate parser dependency.
use crate::{qa_process::Runner, seal::Result};
use std::path::Path;
#[derive(Debug, PartialEq, Eq)]
pub struct Catalog {
    pub translations: Vec<(String, String)>,
    pub plurals: Vec<(String, Vec<String>)>,
}
const MAX_FORMS: usize = 1_048_576;
fn whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
fn text(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn quoted(value: &str) -> Result<String> {
    let value = value.trim_matches(whitespace);
    if !value.starts_with('"') || !value.ends_with('"') {
        return Err("expected quoted PO string".into());
    }
    if value.len() == 1 {
        return Ok(String::new());
    }
    let mut chars = value[1..value.len() - 1].chars();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    _ => {
                        result.push('\\');
                        result.push(next)
                    }
                }
            } else {
                result.push(c)
            }
        } else {
            result.push(c)
        }
    }
    Ok(result)
}
fn set<T>(entries: &mut Vec<(String, T)>, key: String, value: T) {
    if let Some((_, v)) = entries.iter_mut().find(|(k, _)| *k == key) {
        *v = value
    } else {
        entries.push((key, value))
    }
}
#[derive(Default)]
struct Entry {
    id: String,
    plural: String,
    value: String,
    forms: std::collections::BTreeMap<String, String>,
    field: Option<u8>,
    index: String,
}
impl Entry {
    fn finish(&mut self, catalog: &mut Catalog) -> Result<()> {
        if !self.id.is_empty() {
            if !self.plural.is_empty() {
                let max = self
                    .forms
                    .keys()
                    .max_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)))
                    .map(String::as_str)
                    .unwrap_or("0");
                let count = max
                    .parse::<usize>()
                    .ok()
                    .and_then(|v| v.checked_add(1))
                    .filter(|&n| n <= MAX_FORMS)
                    .ok_or("PO entry exceeds explicit plural allocation policy")?;
                let mut forms = Vec::new();
                forms.try_reserve_exact(count)?;
                forms.resize(count, String::new());
                for (index, value) in std::mem::take(&mut self.forms) {
                    forms[index.parse::<usize>()?] = value;
                }
                set(&mut catalog.plurals, std::mem::take(&mut self.id), forms);
            } else {
                set(
                    &mut catalog.translations,
                    std::mem::take(&mut self.id),
                    std::mem::take(&mut self.value),
                );
            }
        }
        *self = Self::default();
        Ok(())
    }
}
fn plural_line(line: &str) -> Result<Option<(String, String)>> {
    let Some(rest) = line.strip_prefix("msgstr[") else {
        return Ok(None);
    };
    let mut end = 0;
    let mut digits = String::new();
    for (at, c) in rest.char_indices() {
        let Some(digit) = crate::argparse_scan::decimal_value(c) else {
            break;
        };
        digits.push(char::from(b'0' + digit));
        end = at + c.len_utf8();
    }
    if digits.is_empty() {
        return Ok(None);
    }
    let Some(tail) = rest[end..].strip_prefix(']') else {
        return Ok(None);
    };
    if !tail.starts_with(whitespace) {
        return Ok(None);
    }
    if digits.len() > 4300 {
        return Err("Python decimal integer digit limit exceeded".into());
    }
    let significant = digits.trim_start_matches('0');
    let index = if significant.is_empty() {
        "0"
    } else {
        significant
    }
    .to_owned();
    Ok(Some((index, quoted(tail)?)))
}
pub fn parse_po(bytes: &[u8]) -> Result<Catalog> {
    let source = text(bytes)?;
    let mut catalog = Catalog {
        translations: vec![],
        plurals: vec![],
    };
    let mut entry = Entry::default();
    for raw in source.split('\n') {
        let line = raw.trim_matches(whitespace);
        if line.is_empty() {
            entry.finish(&mut catalog)?;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("msgid_plural ") {
            entry.plural = quoted(value)?;
            entry.field = Some(1);
            continue;
        }
        if let Some(value) = line.strip_prefix("msgid ") {
            entry.id = quoted(value)?;
            entry.field = Some(0);
            continue;
        }
        if let Some((index, value)) = plural_line(line)? {
            entry.index = index.clone();
            entry.forms.insert(index, value);
            entry.field = Some(3);
            continue;
        }
        if let Some(value) = line.strip_prefix("msgstr ") {
            entry.value = quoted(value)?;
            entry.field = Some(2);
            continue;
        }
        if line.starts_with('"') {
            let value = quoted(line)?;
            match entry.field {
                Some(0) => entry.id.push_str(&value),
                Some(1) => entry.plural.push_str(&value),
                Some(2) => entry.value.push_str(&value),
                Some(3) => entry
                    .forms
                    .get_mut(&entry.index)
                    .ok_or("missing plural continuation state")?
                    .push_str(&value),
                _ => {}
            }
        }
    }
    entry.finish(&mut catalog)?;
    Ok(catalog)
}
pub fn selector(lang: &str, n: i64) -> u8 {
    match lang {
        "en" | "de" | "es" | "tr" => u8::from(n != 1),
        "fr" | "pt-BR" => u8::from(n > 1),
        "uk" => {
            let d = n.rem_euclid(10);
            let h = n.rem_euclid(100);
            if d == 1 && h != 11 {
                0
            } else if (2..=4).contains(&d) && !(12..=14).contains(&h) {
                1
            } else {
                2
            }
        }
        _ => 0,
    }
}

fn lexical(path: &Path) -> std::path::PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut out = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/')
        }
        out.extend(part)
    }
    if out.is_empty() {
        out.push(b'.')
    }
    std::ffi::OsString::from_vec(out).into()
}
use openpilot_log_format::{JsonValue, JsonView};
use std::{fs, io::Write, time::Duration};
#[derive(Clone)]
enum Node {
    Text(Vec<u32>),
    Integer(i64),
    Array(Vec<Node>),
    Object(Vec<(Vec<u32>, Node)>),
}
fn string(value: &str) -> Node {
    Node::Text(value.chars().map(u32::from).collect())
}
fn key(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
impl Node {
    fn encode(&self, utf8: bool, pretty: bool, depth: usize) -> Result<String> {
        match self {
            Self::Text(points) => {
                let node = JsonValue::codepoints(points.clone()).ok_or("invalid Python string")?;
                Ok(if utf8 {
                    node.to_json_utf8()?
                } else {
                    node.to_json()?
                })
            }
            Self::Integer(n) => Ok(n.to_string()),
            Self::Array(values) => {
                if values.is_empty() {
                    return Ok("[]".into());
                }
                let mut items = Vec::new();
                for value in values {
                    let encoded = value.encode(utf8, pretty, depth + 1)?;
                    items.push(if pretty {
                        format!("{}{encoded}", "  ".repeat(depth + 1))
                    } else {
                        encoded
                    });
                }
                Ok(if pretty {
                    format!("[\n{}\n{}]", items.join(",\n"), "  ".repeat(depth))
                } else {
                    format!("[{}]", items.join(","))
                })
            }
            Self::Object(values) => {
                if values.is_empty() {
                    return Ok("{}".into());
                }
                let mut items = Vec::new();
                for (name, value) in values {
                    let name = Self::Text(name.clone()).encode(utf8, false, 0)?;
                    let value = value.encode(utf8, pretty, depth + 1)?;
                    items.push(if pretty {
                        format!("{}{name}: {value}", "  ".repeat(depth + 1))
                    } else {
                        format!("{name}:{value}")
                    });
                }
                Ok(if pretty {
                    format!("{{\n{}\n{}}}", items.join(",\n"), "  ".repeat(depth))
                } else {
                    format!("{{{}}}", items.join(","))
                })
            }
        }
    }
}
fn catalog_node(catalog: Catalog) -> Node {
    Node::Object(vec![
        (
            key("translations"),
            Node::Object(
                catalog
                    .translations
                    .into_iter()
                    .map(|(k, v)| (key(&k), string(&v)))
                    .collect(),
            ),
        ),
        (
            key("plurals"),
            Node::Object(
                catalog
                    .plurals
                    .into_iter()
                    .map(|(k, v)| (key(&k), Node::Array(v.iter().map(|s| string(s)).collect())))
                    .collect(),
            ),
        ),
    ])
}
fn parse_json(source: &str, limit: usize) -> Result<JsonValue> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for ch in source.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            match ch {
                '[' | '{' => {
                    depth += 1;
                    if depth > limit {
                        return Err("Python JSON nesting limit exceeded".into());
                    }
                }
                ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(JsonValue::parse(source)?)
}
fn expected(root: &Path) -> Result<(Node, usize)> {
    let directory = root.join("openpilot/selfdrive/ui/translations");
    let languages = parse_json(&text(&fs::read(directory.join("languages.json"))?)?, 9986)?;
    let JsonView::Object(entries) = languages.view() else {
        return Err("languages must support items".into());
    };
    for (_, value) in entries {
        if matches!(value.view(), JsonView::Array(_) | JsonView::Object(_)) {
            return Err("language code must be hashable".into());
        }
    }
    match fs::read(directory.join("app_en.po")) {
        Ok(bytes) => {
            parse_po(&bytes)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut paths = crate::python_glob::glob(root, &directory, "*.po")?;
    paths.sort_by_key(|path| crate::python_fs::codepoints(path.file_name().unwrap_or_default()));
    let count = paths.len();
    let mut catalogs = vec![];
    for path in paths {
        let name = crate::python_fs::codepoints(path.file_name().ok_or("missing catalog name")?);
        let catalog = parse_po(&fs::read(&path)?)?;
        catalogs.push((name, catalog_node(catalog)));
    }
    let mut selectors = vec![];
    for lang in [
        "en", "de", "fr", "pt-BR", "es", "tr", "uk", "th", "zh-CHT", "zh-CHS", "ko", "ja",
        "missing",
    ] {
        selectors.push((
            key(lang),
            Node::Array(
                (-220..=220)
                    .map(|n| Node::Integer(i64::from(selector(lang, n))))
                    .collect(),
            ),
        ));
    }
    catalogs.push((key("selectors"), Node::Object(selectors)));
    Ok((Node::Object(catalogs), count))
}
pub fn check(root: &Path, binary: &Path, output: &Path, runner: &mut dyn Runner) -> Result<String> {
    fs::create_dir_all(output)?;
    use sha2::{Digest, Sha256};
    if format!(
        "{:x}",
        Sha256::digest(fs::read(root.join("openpilot/system/ui/lib/multilang.py"))?)
    ) != "8ca25630d6be433f0ae8dd7f0708a277d31c873be6669650804f97b45a427f19"
    {
        return Err("multilang reference semantics changed; independently recapture and review native loader".into());
    }
    let (expected, count) = expected(root)?;
    let mut spec = crate::qa_process::CommandSpec::new_os(
        vec![
            lexical(binary).into_os_string(),
            root.join("openpilot/selfdrive/ui/translations")
                .into_os_string(),
        ],
        Duration::MAX,
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    spec.inherit_stdin = true;
    let result = runner.run(&spec)?;
    let stdout = text(&result.stdout)?;
    let _stderr = text(&result.stderr)?;
    if result.code != 0 {
        return Err(format!("translation candidate exited {}", result.code).into());
    }
    let mut source = fs::File::create(output.join("translations-source.json"))?;
    source.write_all(expected.encode(true, true, 0)?.as_bytes())?;
    fs::write(output.join("translations-native.json"), &stdout)?;
    let actual = parse_json(&stdout, 9997)?;
    let reference = JsonValue::parse(&expected.encode(false, false, 0)?)?;
    if !crate::control_tools_policy_check::json::equal(&reference, &actual) {
        return Err("source/native translations differ".into());
    }
    Ok(format!(
        "PASS: {count} complete source/native PO catalogs and 5733 plural selector cases\n"
    ))
}
pub fn cli() -> Result<()> {
    let parsed = crate::argparse_scan::scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            crate::argparse_scan::OptionSpec::value("--binary", true),
            crate::argparse_scan::OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!("usage: check-ui-translations --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = fs::canonicalize(root)?;
    print!(
        "{}",
        check(
            &root,
            Path::new(&parsed.values["--binary"]),
            Path::new(&parsed.values["--output"]),
            &mut crate::qa_process::NativeRunner
        )?
    );
    Ok(())
}
