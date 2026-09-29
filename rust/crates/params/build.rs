use regex::Regex;
use std::{collections::BTreeMap, env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../..");
    let header = fs::read_to_string(root.join("openpilot/common/params.h"))?;
    let source = fs::read_to_string(root.join("openpilot/common/params_keys.h"))?;
    let schema = fs::read_to_string(root.join("openpilot/cereal/log.capnp"))?;
    let enums = Regex::new(r"enum (ParamKeyFlag|ParamKeyType) \{([^}]+)\}")?;
    let item = Regex::new(r"^([A-Z_]+)\s*=\s*(0x[0-9A-Fa-f]+|[0-9]+),?$")?;
    let mut flags = BTreeMap::new();
    let mut types = BTreeMap::new();
    for block in enums.captures_iter(&header) {
        for line in block[2].lines() {
            let line = line.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let entry = item
                .captures(line)
                .ok_or_else(|| format!("unknown params enum: {line}"))?;
            let value = if let Some(hex) = entry[2].strip_prefix("0x") {
                u32::from_str_radix(hex, 16)?
            } else {
                entry[2].parse()?
            };
            let target = if &block[1] == "ParamKeyFlag" {
                &mut flags
            } else {
                &mut types
            };
            target.insert(entry[1].to_owned(), value);
        }
    }
    if flags.is_empty() || types.is_empty() {
        return Err("missing Params enums".into());
    }
    let personality = Regex::new(r"(?s)enum LongitudinalPersonality \{[^}]*standard @([0-9]+);")?;
    let standard = personality
        .captures(&schema)
        .ok_or("missing standard personality")?[1]
        .to_owned();
    let definition =
        Regex::new(r#"^\{"([A-Za-z0-9_]+)", \{([A-Z_ |]+), ([A-Z]+)(?:, (.+))?\}\s*\},$"#)?;
    let mut keys = BTreeMap::new();
    let mut in_keys = false;
    for line in source.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.starts_with("inline static ") {
            in_keys = true;
            continue;
        }
        if !in_keys || line.is_empty() {
            continue;
        }
        if line == "};" {
            in_keys = false;
            continue;
        }
        let entry = definition
            .captures(line)
            .ok_or_else(|| format!("unknown Params definition: {line}"))?;
        let mask = entry[2].split('|').try_fold(0, |mask, flag| {
            flags
                .get(flag.trim())
                .map(|value| mask | value)
                .ok_or("unknown Params flag")
        })?;
        let kind = types.get(&entry[3]).ok_or("unknown Params type")?;
        let default = match entry.get(4).map(|m| m.as_str()) {
            None => "None".to_owned(),
            Some("std::to_string(static_cast<int>(cereal::LongitudinalPersonality::STANDARD))") => {
                format!("Some({standard:?})")
            }
            Some(value)
                if value.starts_with('"') && value.ends_with('"') && !value.contains('\\') =>
            {
                format!("Some({value})")
            }
            Some(value) => return Err(format!("unsupported Params default: {value}").into()),
        };
        let name = &entry[1];
        let value = format!(
            "KeyInfo {{ name: {name:?}, flags: {mask}, kind: {kind}, default: {default} }}"
        );
        if keys.insert(name.to_owned(), value).is_some() {
            return Err(format!("duplicate Params key: {name}").into());
        }
    }
    if in_keys || keys.is_empty() {
        return Err("unterminated or empty Params registry".into());
    }
    let mut output = String::new();
    for (name, value) in flags {
        output.push_str(&format!("pub const {name}: u32 = {value};\n"));
    }
    output.push_str("pub static KEYS: &[KeyInfo] = &[\n");
    for value in keys.values() {
        output.push_str(&format!("{value},\n"));
    }
    output.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("keys.rs"), output)?;
    for path in [
        "openpilot/common/params.h",
        "openpilot/common/params_keys.h",
        "openpilot/cereal/log.capnp",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    Ok(())
}
