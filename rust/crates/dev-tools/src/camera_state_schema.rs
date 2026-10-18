//! Native schema loading: external pinned Cap'n Proto owns the wire oracle;
//! safe Rust reads its real CodeGeneratorRequest to retain pycapnp JSON types.
use super::{Decoded, SchemaDecoder};
use crate::{
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use capnp::schema_capnp::{code_generator_request, field, node, type_};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::HashMap,
    ffi::OsString,
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone)]
enum Type {
    Plain,
    Int64,
    Float,
    Data,
    Struct(u64),
    List(Box<Type>),
    Opaque,
}
struct Field {
    name: String,
    union: bool,
    kind: Type,
}
fn kind(value: type_::Reader<'_>) -> Result<Type> {
    Ok(match value.which()? {
        type_::Int64(()) | type_::Uint64(()) => Type::Int64,
        type_::Float32(()) | type_::Float64(()) => Type::Float,
        type_::Data(()) => Type::Data,
        type_::Struct(value) => Type::Struct(value.get_type_id()),
        type_::List(value) => Type::List(Box::new(kind(value.get_element_type()?)?)),
        type_::AnyPointer(_) | type_::Interface(_) => Type::Opaque,
        _ => Type::Plain,
    })
}
pub struct NativeSchemaDecoder {
    program: PathBuf,
    schema: Option<PathBuf>,
    root: Option<u64>,
    types: HashMap<u64, Vec<Field>>,
}
impl NativeSchemaDecoder {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            schema: None,
            root: None,
            types: HashMap::new(),
        }
    }
    fn command(
        &self,
        args: Vec<OsString>,
        input: Option<Vec<u8>>,
    ) -> Result<crate::qa_process::Captured> {
        let mut argv = vec![self.program.as_os_str().to_owned()];
        argv.extend(args);
        let mut spec = CommandSpec::new_os(argv, Duration::MAX);
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        spec.input = input;
        NativeRunner.run(&spec)
    }
    fn project(
        &self,
        value: &JsonValue,
        kind: &Type,
        path: &mut Vec<String>,
        failure: &mut Option<String>,
    ) -> Result<JsonValue> {
        match kind {
            Type::Int64 => {
                if let Some(text) = value.to_utf8() {
                    Ok(JsonValue::parse(&text)?)
                } else {
                    Ok(value.clone())
                }
            }
            Type::Float => {
                let text = match value.view() {
                    JsonView::Text(_) => value.to_utf8().ok_or("non-Unicode float constant")?,
                    JsonView::Integer(n) => format!("{n}.0"),
                    _ => return Ok(value.clone()),
                };
                Ok(JsonValue::parse(&text)?)
            }
            Type::Data => {
                let image = path.len() == 2
                    && [
                        "roadCameraState",
                        "wideRoadCameraState",
                        "driverCameraState",
                    ]
                    .contains(&path[0].as_str())
                    && path[1] == "image";
                if !image && failure.is_none() {
                    *failure = Some("Object of type bytes is not JSON serializable".into());
                }
                Ok(value.clone())
            }
            Type::Struct(id) => {
                let fields = self.types.get(id).ok_or("missing dynamic struct schema")?;
                let mut out = vec![];
                // pycapnp's to_dict emits its active union first, then nonunion fields.
                for union in [true, false] {
                    for field in fields.iter().filter(|f| f.union == union) {
                        if let Some(child) = value.get(&field.name) {
                            path.push(field.name.clone());
                            let projected = self.project(&child, &field.kind, path, failure)?;
                            path.pop();
                            out.push(format!(
                                "{}: {}",
                                JsonValue::text(&field.name).to_json()?,
                                projected.to_json()?
                            ));
                        }
                    }
                }
                Ok(JsonValue::parse(&format!("{{{}}}", out.join(", ")))?)
            }
            Type::List(element) => {
                let JsonView::Array(items) = value.view() else {
                    return Err("native schema list was not an array".into());
                };
                let mut out = vec![];
                for (index, item) in items.into_iter().enumerate() {
                    path.push(index.to_string());
                    out.push(self.project(&item, element, path, failure)?.to_json()?);
                    path.pop();
                }
                Ok(JsonValue::parse(&format!("[{}]", out.join(", ")))?)
            }
            Type::Opaque => {
                if failure.is_none() {
                    *failure =
                        Some("Object of type _DynamicObjectReader is not JSON serializable".into());
                }
                Ok(value.clone())
            }
            Type::Plain => Ok(value.clone()),
        }
    }
}
impl SchemaDecoder for NativeSchemaDecoder {
    fn load(&mut self, path: &Path) -> Result<()> {
        let schema = resolve_non_strict(path)?;
        let parent = schema.parent().ok_or("schema has no directory")?;
        let mut include = OsString::from("-I");
        include.push(parent);
        let output = self.command(
            vec![
                "compile".into(),
                include,
                "-o-".into(),
                schema.as_os_str().to_owned(),
            ],
            None,
        )?;
        if output.code != 0 {
            return Err(format!(
                "native schema compiler failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let message = capnp::serialize::read_message(
            Cursor::new(&output.stdout),
            capnp::message::ReaderOptions::new(),
        )?;
        let request = message.get_root::<code_generator_request::Reader<'_>>()?;
        let nodes = request.get_nodes()?;
        let requested = request.get_requested_files()?;
        if requested.is_empty() {
            return Err("schema compiler did not identify a file".into());
        }
        let file_id = requested.get(0).get_id();
        let mut root = None;
        let mut types = HashMap::new();
        for value in nodes {
            if value.get_id() == file_id {
                for nested in value.get_nested_nodes()? {
                    if nested.get_name()?.to_str()? == "Event" {
                        root = Some(nested.get_id());
                    }
                }
            }
            if let node::Struct(structure) = value.which()? {
                let mut fields = vec![];
                for value in structure.get_fields()? {
                    let field_type = match value.which()? {
                        field::Slot(slot) => kind(slot.get_type()?)?,
                        field::Group(group) => Type::Struct(group.get_type_id()),
                    };
                    fields.push(Field {
                        name: value.get_name()?.to_str()?.to_owned(),
                        union: value.get_discriminant_value() != u16::MAX,
                        kind: field_type,
                    });
                }
                types.insert(value.get_id(), fields);
            }
        }
        self.root = root;
        self.types = types;
        self.schema = Some(schema);
        Ok(())
    }
    fn decode(&mut self, wire: &[u8]) -> Result<Decoded> {
        // The original loads any valid schema, then looks up Event.from_bytes
        // only for a row containing wire. Preserve that failure/artifact phase.
        let root = self.root.ok_or("schema has no Event type")?;
        if !self.types.contains_key(&root) {
            return Err("Event is not a struct".into());
        }
        if !wire.len().is_multiple_of(8) {
            return Err("input length must be a multiple of eight bytes".into());
        }
        // from_bytes reads only the first message, ignoring complete trailing words.
        let mut cursor = Cursor::new(wire);
        let _ = capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::new())?;
        let consumed = usize::try_from(cursor.position())?;
        let schema = self.schema.as_ref().ok_or("schema not loaded")?;
        let mut include = OsString::from("-I");
        include.push(schema.parent().ok_or("schema has no parent")?);
        let output = self.command(
            vec![
                "convert".into(),
                include,
                "--short".into(),
                "binary:json".into(),
                schema.as_os_str().to_owned(),
                "Event".into(),
            ],
            Some(wire[..consumed].to_vec()),
        )?;
        if output.code != 0 {
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            if diagnostic.contains("don't know how to JSON-encode AnyPointer") {
                return Ok(Decoded {
                    value: JsonValue::parse("null")?,
                    serialization_error: Some(
                        "Object of type _DynamicObjectReader is not JSON serializable".into(),
                    ),
                });
            }
            return Err(format!("native schema decode failed: {diagnostic}").into());
        }
        let text = String::from_utf8(output.stdout)?;
        // The shared JSON parser canonicalizes the integer token -0 to 0.
        // Cap'n Proto prints a floating negative zero as -0, so retain its sign
        // before parsing, without touching quoted text or any other number.
        let text = preserve_negative_zero(&text);
        let value = json::parse_text(&text)?;
        let mut serialization_error = None;
        let value = self.project(
            &value,
            &Type::Struct(root),
            &mut vec![],
            &mut serialization_error,
        )?;
        Ok(Decoded {
            value,
            serialization_error,
        })
    }
}

fn preserve_negative_zero(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut i = 0;
    let mut quoted = false;
    let mut escaped = false;
    while i < bytes.len() {
        let byte = bytes[i];
        if quoted {
            let ch = text[i..].chars().next().unwrap();
            output.push(ch);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            i += ch.len_utf8();
            continue;
        }
        if byte == b'"' {
            quoted = true;
        }
        if byte == b'-'
            && bytes.get(i + 1) == Some(&b'0')
            && bytes
                .get(i + 2)
                .is_none_or(|b| matches!(b, b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t'))
        {
            output.push_str("-0.0");
            i += 2;
            continue;
        }
        // Native JSON may contain UTF-8; append whole scalar rather than byte-casting.
        let ch = text[i..].chars().next().unwrap();
        output.push(ch);
        i += ch.len_utf8();
    }
    output
}
