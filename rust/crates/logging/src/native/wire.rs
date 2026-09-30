use crate::{record::Level, site::Site, Error};
use serde::Serialize;
use serde_json::{ser::Formatter, Value};
use std::{
    collections::BTreeMap,
    env,
    io::{self, Write},
};

pub(super) type Context = BTreeMap<&'static str, Value>;
pub(super) fn c_string(text: &str) -> &str {
    text.split('\0').next().unwrap_or("")
}
pub(super) fn context(version: &str, device: &str) -> Result<Context, Error> {
    let mut context = Context::new();
    for (source, key) in [
        ("DONGLE_ID", "dongle_id"),
        ("GIT_ORIGIN", "origin"),
        ("GIT_BRANCH", "branch"),
        ("GIT_COMMIT", "commit"),
        ("MANAGER_DAEMON", "daemon"),
    ] {
        match env::var(source) {
            Ok(value) => {
                context.insert(key, Value::String(value));
            }
            Err(env::VarError::NotPresent) => {}
            Err(env::VarError::NotUnicode(_)) => {
                return Err(Error::Contract("native logging context is not UTF-8"))
            }
        }
    }
    context.insert("version", c_string(version).into());
    context.insert("device", device.into());
    context.insert("dirty", env::var_os("CLEAN").is_none().into());
    context.insert("runtime_language", "rust".into());
    context.insert(
        "source_commit",
        env!("OPENPILOT_LOGGING_SOURCE_COMMIT").into(),
    );
    context.insert("source_tree", env!("OPENPILOT_LOGGING_SOURCE_TREE").into());
    Ok(context)
}

pub(super) fn packet(
    site: Site,
    level: Level,
    text: &str,
    created: f64,
    context: &Context,
) -> Result<Vec<u8>, Error> {
    // json11 sorts object keys, emits UTF-8, and inserts spaces after commas and colons.
    #[derive(Serialize)]
    struct Record<'a> {
        created: f64,
        ctx: &'a Context,
        filename: &'a str,
        funcname: &'a str,
        levelnum: u8,
        lineno: u32,
        msg: &'a str,
    }
    let record = Record {
        created,
        ctx: context,
        filename: c_string(site.file),
        funcname: c_string(site.function),
        levelnum: level as u8,
        lineno: site.line,
        msg: text,
    };
    let mut packet = vec![level as u8];
    record
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut packet,
            Json11,
        ))
        .map_err(|_| Error::Contract("native log serialization failed"))?;
    Ok(packet)
}
struct Json11;
impl Formatter for Json11 {
    fn begin_object_key<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }
    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        for part in fragment.split_inclusive(['\u{2028}', '\u{2029}']) {
            match part.chars().last() {
                Some('\u{2028}') => {
                    writer.write_all(part.trim_end_matches('\u{2028}').as_bytes())?;
                    writer.write_all(b"\\u2028")?;
                }
                Some('\u{2029}') => {
                    writer.write_all(part.trim_end_matches('\u{2029}').as_bytes())?;
                    writer.write_all(b"\\u2029")?;
                }
                _ => writer.write_all(part.as_bytes())?,
            }
        }
        Ok(())
    }
}
