use crate::Error;
use capnp::{dynamic_value, introspect::TypeVariant};
pub fn enum_field<'a>(
    target: impl Into<dynamic_value::Builder<'a>>,
    name: &str,
    value: u32,
) -> Result<(), Error> {
    let dynamic_value::Builder::Struct(mut target) = target.into() else {
        return Err(Error::Protocol("enum target is not a struct"));
    };
    let field = target.get_schema().get_field_by_name(name)?;
    let TypeVariant::Enum(schema) = field.get_type().which() else {
        return Err(Error::Protocol("enum field schema mismatch"));
    };
    target.set(
        field,
        dynamic_value::Reader::Enum(dynamic_value::Enum::new(
            u16::try_from(value).map_err(|_| Error::Protocol("enum ordinal out of range"))?,
            schema.into(),
        )),
    )?;
    Ok(())
}
