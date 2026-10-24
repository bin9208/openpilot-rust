//! Native export of the seven documented-car lists from the current opendbc
//! platform declarations. This parses only the source shape used by these
//! seven brands; an unfamiliar declaration fails instead of copying old JSON.
use crate::{
    argparse_scan::{self, OptionSpec},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub const BRANDS: [&str; 7] = [
    "hyundai",
    "gm",
    "toyota",
    "mazda",
    "ford",
    "volkswagen",
    "tesla",
];
const MAX_SOURCE: u64 = 2 * 1024 * 1024;
const HISTORICAL_GENERATOR: &str = "rust/tools/generate_manager_cars.py";
const HISTORICAL_GENERATOR_SHA256: &str =
    "a090e5ba6e30856c4b072ed2d04e682971172fd096592693a0a98756ca9f4caf";
// Source-only contract for the supported opendbc declaration subset. Changing
// any of these definitions requires reviewing the parser and a fresh original
// generator comparison; matching today's literal car names is insufficient.
const SOURCE_PINS: [(&str, &str); 9] = [
    (
        "opendbc/car/__init__.py",
        "f56a23742d37395f5869f9c63a367375e038e2b47966efaed7a9679307c01e5e",
    ),
    (
        "opendbc/car/docs_definitions.py",
        "00801f31f5a245a27be1bb38865cad2270468f2a226bbba36ddb0a60ec4100ac",
    ),
    (
        "opendbc/car/hyundai/values.py",
        "53a8c9f184653d1dc48c3fcd1b05ac189323d54eaeb41de8302290faec63bfa8",
    ),
    (
        "opendbc/car/gm/values.py",
        "228d20a54736a99113d1968e121d1a6951ab0a7a9686a7e7ecac052a68b2a7bf",
    ),
    (
        "opendbc/car/toyota/values.py",
        "dbe9030ee5509f202989d7749d77cc62fdcd20ab07a55391b0a516d154145070",
    ),
    (
        "opendbc/car/mazda/values.py",
        "6eb75330447d850d76604916a1c213e3b95199012b1c4fbf037e3bf07cb0390a",
    ),
    (
        "opendbc/car/ford/values.py",
        "816861eb35d11d0ccd154dc1ed0b42cc76aebde49848c8cf06494039fbca38e9",
    ),
    (
        "opendbc/car/volkswagen/values.py",
        "ee5a875a5c7ef292710c1d9017d557cd6e9da2d5b08056bfdf9cc226f7951dea",
    ),
    (
        "opendbc/car/tesla/values.py",
        "6027321c0be5847494f7062f0b2be0a09595d5b94c4d482a01f176545fde7bb4",
    ),
];

fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

type Identity = (u64, u64, u64, u32, (i64, i64), (i64, i64));
fn identity(meta: &fs::Metadata) -> Identity {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mode(),
        (meta.mtime(), meta.mtime_nsec()),
        (meta.ctime(), meta.ctime_nsec()),
    )
}

struct Held {
    path: PathBuf,
    file: File,
    identity: Identity,
    digest: String,
    limit: u64,
}

impl Held {
    fn open(path: PathBuf, limit: u64) -> Result<Self> {
        let before = fs::symlink_metadata(&path)?;
        require(
            before.is_file() && !before.file_type().is_symlink(),
            format!(
                "manager car source is not a regular file: {}",
                path.display()
            ),
        )?;
        require(
            before.len() <= limit,
            "manager car source exceeds read bound",
        )?;
        let file = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let meta = file.metadata()?;
        require(
            identity(&before) == identity(&meta),
            "manager car source changed while opening",
        )?;
        let mut held = Self {
            path,
            file,
            identity: identity(&meta),
            digest: String::new(),
            limit,
        };
        held.digest = sha(&held.read()?);
        held.verify()?;
        Ok(held)
    }

    fn read(&self) -> Result<Vec<u8>> {
        let mut reader = self.file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        reader.take(self.limit + 1).read_to_end(&mut bytes)?;
        require(
            bytes.len() as u64 == self.identity.2 && bytes.len() as u64 <= self.limit,
            "manager car source length changed",
        )?;
        Ok(bytes)
    }

    fn verify(&self) -> Result<()> {
        require(
            identity(&self.file.metadata()?) == self.identity
                && identity(&fs::symlink_metadata(&self.path)?) == self.identity,
            format!(
                "manager car source identity changed: {}",
                self.path.display()
            ),
        )?;
        if !self.digest.is_empty() {
            require(
                sha(&self.read()?) == self.digest,
                format!(
                    "manager car source content changed: {}",
                    self.path.display()
                ),
            )?;
        }
        Ok(())
    }
}

fn pinned_source(root: &Path, name: &str) -> Result<Held> {
    let expected = SOURCE_PINS
        .iter()
        .find(|(path, _)| *path == name)
        .map(|(_, digest)| *digest)
        .ok_or("manager car source pin missing")?;
    let held = Held::open(root.join(name), MAX_SOURCE)?;
    require(
        held.digest == expected,
        format!("manager car source revision changed: {name}"),
    )?;
    Ok(held)
}

type DirectoryIdentity = (u64, u64, u32);

fn directory_identity(meta: &fs::Metadata) -> DirectoryIdentity {
    (meta.dev(), meta.ino(), meta.mode())
}

struct HistoricalGenerator {
    parent: File,
    parent_path: PathBuf,
    parent_identity: DirectoryIdentity,
    path: PathBuf,
    held: Option<Held>,
}

impl HistoricalGenerator {
    fn open(root: &Path) -> Result<Self> {
        let parent_path = root.join("rust/tools");
        let parent = File::from(rustix::fs::open(
            &parent_path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let parent_identity = directory_identity(&parent.metadata()?);
        require(
            directory_identity(&fs::symlink_metadata(&parent_path)?) == parent_identity,
            "manager car historical parent changed while opening",
        )?;
        let path = root.join(HISTORICAL_GENERATOR);
        let held = match fs::symlink_metadata(&path) {
            Ok(_) => {
                let held = Held::open(path.clone(), MAX_SOURCE)?;
                require(
                    held.digest == HISTORICAL_GENERATOR_SHA256,
                    "manager car historical generator source changed",
                )?;
                Some(held)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let result = Self {
            parent,
            parent_path,
            parent_identity,
            path,
            held,
        };
        result.verify()?;
        Ok(result)
    }

    fn verify(&self) -> Result<()> {
        for current in [
            self.parent.metadata()?,
            fs::symlink_metadata(&self.parent_path)?,
        ] {
            require(
                current.is_dir() && directory_identity(&current) == self.parent_identity,
                "manager car historical parent identity changed",
            )?;
        }
        if let Some(held) = &self.held {
            held.verify()?;
        } else {
            require(
                fs::symlink_metadata(&self.path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "manager car historical generator reappeared",
            )?;
        }
        Ok(())
    }
}

fn skip_space_and_comments(bytes: &[u8], mut pos: usize) -> usize {
    loop {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if bytes.get(pos) != Some(&b'#') {
            return pos;
        }
        while pos < bytes.len() && bytes[pos] != b'\n' {
            pos += 1;
        }
    }
}

fn literal(bytes: &[u8], start: usize) -> Result<(String, usize)> {
    let quote = *bytes.get(start).ok_or("missing Python string literal")?;
    require(
        quote == b'\'' || quote == b'"',
        "manager car name is not a literal string",
    )?;
    let mut output = Vec::new();
    let mut pos = start + 1;
    while pos < bytes.len() {
        let byte = bytes[pos];
        pos += 1;
        if byte == quote {
            return Ok((String::from_utf8(output)?, pos));
        }
        if byte != b'\\' {
            output.push(byte);
            continue;
        }
        let escaped = *bytes.get(pos).ok_or("truncated manager car name escape")?;
        pos += 1;
        match escaped {
            b'\\' | b'\'' | b'"' => output.push(escaped),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'x' | b'u' => {
                let width = if escaped == b'x' { 2 } else { 4 };
                let hex = std::str::from_utf8(
                    bytes
                        .get(pos..pos + width)
                        .ok_or("truncated manager car Unicode escape")?,
                )?;
                let scalar = u32::from_str_radix(hex, 16)?;
                let character =
                    char::from_u32(scalar).ok_or("invalid manager car Unicode escape")?;
                output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                pos += width;
            }
            _ => return Err("unsupported manager car name escape".into()),
        }
    }
    Err("unterminated manager car name".into())
}

fn matching(bytes: &[u8], open: usize, left: u8, right: u8) -> Result<usize> {
    require(
        bytes.get(open) == Some(&left),
        "manager platform argument shape changed",
    )?;
    let mut depth = 0usize;
    let mut pos = open;
    while pos < bytes.len() {
        match bytes[pos] {
            b'\'' | b'"' => {
                pos = literal(bytes, pos)?.1;
                continue;
            }
            b'#' => {
                while pos < bytes.len() && bytes[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            byte if byte == left => depth += 1,
            byte if byte == right => {
                depth -= 1;
                if depth == 0 {
                    return Ok(pos);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    Err("unterminated manager platform expression".into())
}

fn identifier(bytes: &[u8], pos: usize) -> Option<(&str, usize)> {
    let first = *bytes.get(pos)?;
    if !first.is_ascii_alphabetic() && first != b'_' {
        return None;
    }
    let mut end = pos + 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    Some((std::str::from_utf8(&bytes[pos..end]).ok()?, end))
}

#[derive(Clone)]
struct Doc {
    name: String,
    hybrid: bool,
    plug_in_hybrid: bool,
}

fn named_flag(call: &[u8], name: &str) -> Result<bool> {
    let mut pos = 0;
    while pos < call.len() {
        match call[pos] {
            b'\'' | b'"' => {
                pos = literal(call, pos)?.1;
                continue;
            }
            b'#' => {
                while pos < call.len() && call[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            _ => {}
        }
        if let Some((key, end)) = identifier(call, pos) {
            pos = end;
            if key != name {
                continue;
            }
            let eq = skip_space_and_comments(call, end);
            if call.get(eq) != Some(&b'=') {
                continue;
            }
            let value = skip_space_and_comments(call, eq + 1);
            let (flag, _) =
                identifier(call, value).ok_or("unsupported manager car documentation flag")?;
            return match flag {
                "True" => Ok(true),
                "False" => Ok(false),
                _ => Err("unsupported manager car documentation flag".into()),
            };
        }
        pos += 1;
    }
    Ok(false)
}

fn docs_in_list(list: &[u8]) -> Result<Vec<Doc>> {
    let mut docs = Vec::new();
    let mut pos = skip_space_and_comments(list, 0);
    while pos < list.len() {
        let (constructor, end) =
            identifier(list, pos).ok_or("unsupported manager car documentation expression")?;
        require(
            constructor.ends_with("Docs") || constructor.contains("DocsHW"),
            format!("unsupported manager car documentation constructor: {constructor}"),
        )?;
        let open = skip_space_and_comments(list, end);
        let close = matching(list, open, b'(', b')')?;
        let first = skip_space_and_comments(list, open + 1);
        let (name, _) = literal(list, first)?;
        let call = &list[first..close];
        docs.push(Doc {
            name,
            hybrid: named_flag(call, "hybrid")?,
            plug_in_hybrid: named_flag(call, "plug_in_hybrid")?,
        });
        pos = skip_space_and_comments(list, close + 1);
        if pos < list.len() {
            require(
                list[pos] == b',',
                "manager car documentation expression is not a list item",
            )?;
            pos = skip_space_and_comments(list, pos + 1);
        }
    }
    Ok(docs)
}

fn split_name(name: &str) -> Result<(&str, &str, &str)> {
    let (make, model) = name
        .split_once(' ')
        .ok_or("manager Ford car name has no model")?;
    let years = regex::Regex::new(r" \d{4}(?:-\d{2})?(?:,|$)")?;
    if let Some(found) = years.find(model) {
        Ok((make, &model[..found.start()], &model[found.start() + 1..]))
    } else {
        Ok((make, model, ""))
    }
}

fn platform_names(brand: &str, source: &str) -> Result<Vec<String>> {
    let (_, class) = source
        .split_once("\nclass CAR(Platforms):")
        .ok_or("manager car source lacks class CAR(Platforms)")?;
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in class.split_inclusive('\n') {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            offset += line.len();
            continue;
        }
        if !line.starts_with(' ') {
            break;
        }
        if line.starts_with("  ") && !line.starts_with("   ") {
            if let Some((member, constructor)) = line[2..].split_once(" = ") {
                if member
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
                    && constructor.contains('(')
                {
                    starts.push(offset + 2 + member.len() + 3);
                }
            }
        }
        offset += line.len();
    }
    require(
        !starts.is_empty(),
        format!("manager {brand} platform roster missing"),
    )?;
    let mut names = Vec::new();
    for start in starts {
        let bytes = class.as_bytes();
        let (constructor, end) =
            identifier(bytes, start).ok_or("manager platform constructor changed")?;
        require(
            constructor.ends_with("PlatformConfig") || constructor == "FordF150LightningPlatform",
            format!("unsupported manager platform constructor: {constructor}"),
        )?;
        let open = skip_space_and_comments(bytes, end);
        require(
            bytes.get(open) == Some(&b'('),
            "manager platform constructor call missing",
        )?;
        let first = skip_space_and_comments(bytes, open + 1);
        let close = matching(bytes, first, b'[', b']')?;
        let mut docs = docs_in_list(&bytes[first + 1..close])?;
        if constructor == "ToyotaSecOCPlatformConfig" || constructor == "FordF150LightningPlatform"
        {
            docs.clear();
        }
        let original_count = docs.len();
        for doc in &docs {
            names.push(doc.name.clone());
        }
        if brand == "ford" {
            for doc in docs.iter().take(original_count) {
                let (make, model, years) = split_name(&doc.name)?;
                if doc.hybrid {
                    names.push(format!("{make} {model} Hybrid {years}"));
                }
                if doc.plug_in_hybrid {
                    names.push(format!("{make} {model} Plug-in Hybrid {years}"));
                }
            }
        }
    }
    Ok(names)
}

fn source_names(brand: &str, held: &Held) -> Result<Vec<String>> {
    let bytes = held.read()?;
    require(
        sha(&bytes) == held.digest,
        "manager car source changed while parsing",
    )?;
    let source = std::str::from_utf8(&bytes)?;
    if brand == "ford" {
        require(
            source.contains("self.car_docs.append(replace(copy.deepcopy(car_docs), name=name))")
                && source.contains("self.car_docs = []"),
            "Ford documentation expansion changed",
        )?;
    }
    if brand == "toyota" {
        require(
            source.contains("class ToyotaSecOCPlatformConfig(PlatformConfig):")
                && source.contains("self.car_docs = []"),
            "Toyota documentation suppression changed",
        )?;
    }
    platform_names(brand, source)
}

fn python_json(names: &[String]) -> Result<Vec<u8>> {
    let mut text = String::from("[");
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(&serde_json::to_string(name)?);
    }
    text.push_str("]\n");
    Ok(text.into_bytes())
}

pub struct BrandResult {
    pub brand: &'static str,
    pub count: usize,
    pub source_sha256: String,
}

pub fn generate_with_emit<F>(
    root: &Path,
    output: &Path,
    check: bool,
    mut emit: F,
) -> Result<Vec<BrandResult>>
where
    F: FnMut(&str, usize) -> Result<()>,
{
    let historical = HistoricalGenerator::open(root)?;
    let platform_definitions = pinned_source(root, "opendbc/car/__init__.py")?;
    let documentation_definitions = pinned_source(root, "opendbc/car/docs_definitions.py")?;
    require(
        std::str::from_utf8(&platform_definitions.read()?)?.contains("class Platforms(")
            && std::str::from_utf8(&documentation_definitions.read()?)?.contains("def split_name("),
        "manager car source definitions changed",
    )?;
    let held = BRANDS
        .iter()
        .map(|brand| pinned_source(root, &format!("opendbc/car/{brand}/values.py")))
        .collect::<Result<Vec<_>>>()?;
    historical.verify()?;
    fs::create_dir_all(output)?;
    let mut results = Vec::new();
    for (brand, source) in BRANDS.into_iter().zip(&held) {
        let names = source_names(brand, source)?;
        let text = python_json(&names)?;
        let target = output.join(format!("{brand}.json"));
        if check {
            let existing = fs::read_to_string(&target)?;
            let normalized = existing.replace("\r\n", "\n").replace('\r', "\n");
            require(
                normalized.as_bytes() == text,
                format!("{brand} source names changed"),
            )?;
        } else {
            fs::write(&target, &text)?;
        }
        source.verify()?;
        platform_definitions.verify()?;
        documentation_definitions.verify()?;
        historical.verify()?;
        emit(brand, names.len())?;
        historical.verify()?;
        results.push(BrandResult {
            brand,
            count: names.len(),
            source_sha256: source.digest.clone(),
        });
    }
    for source in &held {
        source.verify()?;
    }
    platform_definitions.verify()?;
    documentation_definitions.verify()?;
    historical.verify()?;
    Ok(results)
}

pub fn generate(root: &Path, output: &Path, check: bool) -> Result<Vec<BrandResult>> {
    generate_with_emit(root, output, check, |_, _| Ok(()))
}

pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed =
        argparse_scan::scan_os_with_positionals(&args, &[OptionSpec::flag("--check", None)], 1)?;
    if parsed.help {
        std::io::stdout()
            .lock()
            .write_all(b"usage: generate-manager-cars [-h] [--check] output\n")?;
        return Ok(());
    }
    let root = std::env::current_dir()?;
    let output = PathBuf::from(&parsed.positionals[0]);
    generate_with_emit(
        &root,
        &output,
        parsed.values.contains_key("--check"),
        |brand, count| {
            writeln!(
                std::io::stdout().lock(),
                "{brand}: {count} source car documentation names"
            )?;
            Ok(())
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn historical_generator_absence_reappearance_and_parent_replacement() {
        let root = tempfile::tempdir().unwrap();
        let tools = root.path().join("rust/tools");
        fs::create_dir_all(&tools).unwrap();
        let source = tools.join("generate_manager_cars.py");
        let absent = HistoricalGenerator::open(root.path()).unwrap();
        absent.verify().unwrap();
        fs::write(&source, b"unexpected reappearance").unwrap();
        assert!(absent.verify().is_err());
        assert!(HistoricalGenerator::open(root.path()).is_err());
        fs::remove_file(&source).unwrap();
        symlink("missing-target", &source).unwrap();
        assert!(HistoricalGenerator::open(root.path()).is_err());
        fs::remove_file(&source).unwrap();
        let restored = HistoricalGenerator::open(root.path()).unwrap();
        fs::rename(&tools, root.path().join("rust/old-tools")).unwrap();
        fs::create_dir(&tools).unwrap();
        assert!(restored.verify().is_err());
    }

    #[test]
    fn ford_expansion_preserves_platform_then_variant_order() {
        let source = r#"
class CAR(Platforms):
  FIRST = FordPlatformConfig(
    [FordCarDocs("Ford Escape 2020-22", hybrid=True, plug_in_hybrid=True),
     FordCarDocs("Lincoln Aviator 2024", plug_in_hybrid=True)],
    CarSpecs(mass=1),
  )
  SECOND = FordF150LightningPlatform(
    [FordCarDocs("Ford F-150 Lightning 2022-23")], CarSpecs(mass=2))
  THIRD = FordCANFDPlatformConfig(
    [FordCarDocs("Ford Ranger 2024")], CarSpecs(mass=3))
"#;
        assert_eq!(
            platform_names("ford", source).unwrap(),
            [
                "Ford Escape 2020-22",
                "Lincoln Aviator 2024",
                "Ford Escape Hybrid 2020-22",
                "Ford Escape Plug-in Hybrid 2020-22",
                "Lincoln Aviator Plug-in Hybrid 2024",
                "Ford Ranger 2024",
            ]
        );
    }

    #[test]
    fn unsupported_documentation_expression_and_truncated_literal_fail_closed() {
        let unsupported =
            "\nclass CAR(Platforms):\n  A = PlatformConfig([CarDocs(make_name())], CarSpecs())\n";
        assert!(platform_names("gm", unsupported).is_err());
        let truncated =
            "\nclass CAR(Platforms):\n  A = PlatformConfig([CarDocs(\"Unclosed)], CarSpecs())\n";
        assert!(platform_names("gm", truncated).is_err());
        let unexpected = "\nclass CAR(Platforms):\n  A = UnknownConfig([CarDocs(\"Example 2024\")], CarSpecs())\n";
        assert!(platform_names("gm", unexpected).is_err());
        let additional = "\nclass CAR(Platforms):\n  A = PlatformConfig([CarDocs(\"Example 2024\"), make_docs(\"Hidden 2025\")], CarSpecs())\n";
        assert!(platform_names("gm", additional).is_err());
        let splat = "\nclass CAR(Platforms):\n  A = PlatformConfig([CarDocs(\"Example 2024\"), *MORE_DOCS], CarSpecs())\n";
        assert!(platform_names("gm", splat).is_err());
    }

    #[test]
    fn toyota_secoc_is_not_exported_and_unicode_is_not_ascii_escaped() {
        let source = "\nclass CAR(Platforms):\n  A = ToyotaSecOCPlatformConfig([ToyotaCarDocs(\"Toyota Hidden 2024\")], CarSpecs())\n  B = PlatformConfig([ToyotaCarDocs(\"Toyota Café 2024\")], CarSpecs())\n";
        let names = platform_names("toyota", source).unwrap();
        assert_eq!(names, ["Toyota Café 2024"]);
        assert_eq!(
            python_json(&names).unwrap(),
            "[\"Toyota Café 2024\"]\n".as_bytes()
        );
    }
}
