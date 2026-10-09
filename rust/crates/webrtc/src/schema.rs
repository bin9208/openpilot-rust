//! Port of system/webrtc/schema.py using the full generated Cereal schema.
use crate::Error;
use capnp::{
    introspect::{Introspect, Type, TypeVariant},
    schema::StructSchema,
};
use openpilot_cereal::log_capnp::event;
use serde_json::{Map, Value};

pub(crate) fn event_schema() -> Result<StructSchema, Error> {
    match event::Owned::introspect().which() {
        TypeVariant::Struct(schema) => Ok(schema.into()),
        _ => Err(Error::Contract("Event must be a struct")),
    }
}

fn struct_schema(schema: StructSchema) -> Result<Value, Error> {
    let mut result = Map::new();
    for field in schema.get_fields()? {
        let name = field.get_proto().get_name()?.to_str()?;
        if !name.ends_with("DEPRECATED") && name != "deprecated" {
            let value = match field.get_proto().which()? {
                capnp::schema_capnp::field::Slot(slot)
                    if matches!(
                        slot.get_type()?.which()?,
                        capnp::schema_capnp::type_::AnyPointer(_)
                    ) =>
                {
                    Value::String("anyPointer".to_owned())
                }
                capnp::schema_capnp::field::Slot(_) | capnp::schema_capnp::field::Group(_) => {
                    type_schema(field.get_type())?
                }
            };
            result.insert(name.to_owned(), value);
        }
    }
    Ok(Value::Object(result))
}

fn type_schema(kind: Type) -> Result<Value, Error> {
    let scalar = match kind.which() {
        TypeVariant::Struct(schema) => return struct_schema(schema.into()),
        TypeVariant::List(element) => return Ok(Value::Array(vec![type_schema(element)?])),
        TypeVariant::Void => "void",
        TypeVariant::Bool => "bool",
        TypeVariant::Int8 => "int8",
        TypeVariant::Int16 => "int16",
        TypeVariant::Int32 => "int32",
        TypeVariant::Int64 => "int64",
        TypeVariant::UInt8 => "uint8",
        TypeVariant::UInt16 => "uint16",
        TypeVariant::UInt32 => "uint32",
        TypeVariant::UInt64 => "uint64",
        TypeVariant::Float32 => "float32",
        TypeVariant::Float64 => "float64",
        TypeVariant::Text | TypeVariant::Enum(_) => "text",
        TypeVariant::Data => "data",
        TypeVariant::AnyPointer => "anyPointer",
        TypeVariant::Capability => "interface",
    };
    Ok(Value::String(scalar.to_owned()))
}

/// Returns the original /schema response for the selected Event fields.
///
/// # Errors
/// Rejects missing and deprecated services or malformed schema metadata.
pub fn services(names: &[&str]) -> Result<Value, Error> {
    let schema = event_schema()?;
    let mut result = Map::new();
    for name in names.iter().filter(|name| !name.is_empty()) {
        if name.ends_with("DEPRECATED") {
            return Err(Error::Service((*name).to_owned()));
        }
        let field = schema
            .find_field_by_name(name)?
            .ok_or_else(|| Error::Service((*name).to_owned()))?;
        result.insert((*name).to_owned(), type_schema(field.get_type())?);
    }
    Ok(Value::Object(result))
}
