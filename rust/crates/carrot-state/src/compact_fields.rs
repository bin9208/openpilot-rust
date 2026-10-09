use super::value;
use crate::Error;
use capnp::dynamic_value::Reader;
use num_traits::ToPrimitive;
pub(super) struct Field {
    pub path: &'static [&'static str],
    pub spec: Spec,
}
pub(super) enum Spec {
    Bool,
    I8,
    I16,
    U16,
    I32,
    U32,
    U64,
    F32,
    F64,
    Text,
    CoordList,
    F32List,
    F32FirstList,
    I16List,
    U16CmList,
    I16CmList,
    I16MmList,
    Enum(&'static [&'static str]),
    Struct(&'static [Field]),
    StructList(&'static [Field]),
}
fn finite(value: Reader<'_>, default: f64) -> f64 {
    let number = value::number(value, default);
    if number.is_finite() {
        number
    } else {
        default
    }
}
fn float(out: &mut Vec<u8>, number: f64) -> Result<(), Error> {
    let number = number
        .to_f32()
        .filter(|number| number.is_finite())
        .ok_or_else(|| Error::Source("float too large to pack with f format".into()))?;
    out.extend(number.to_le_bytes());
    Ok(())
}
fn list(value: Reader<'_>, limit: u32) -> Vec<Reader<'_>> {
    let Reader::List(value) = value else {
        return Vec::new();
    };
    (0..value.len().min(limit))
        .map(|index| value.get(index))
        .collect::<Result<_, _>>()
        .unwrap_or_default()
}
pub(super) fn fields(out: &mut Vec<u8>, value: Reader<'_>, schema: &[Field]) -> Result<(), Error> {
    for entry in schema {
        pack(out, value::path(value, entry.path), &entry.spec)?;
    }
    Ok(())
}
fn pack(out: &mut Vec<u8>, value: Reader<'_>, spec: &Spec) -> Result<(), Error> {
    let integer = value::integer(value);
    match spec {
        Spec::Bool => out.push(u8::from(value::truth(value))),
        Spec::I8 => out.extend(
            i8::try_from(integer.clamp(-128, 127))
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Spec::I16 => out.extend(
            i16::try_from(integer.clamp(-32768, 32767))
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Spec::U16 => out.extend(
            u16::try_from(integer.clamp(0, 65535))
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Spec::I32 => out.extend(
            i32::try_from(integer.clamp(i128::from(i32::MIN), i128::from(i32::MAX)))
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Spec::U32 => out.extend(
            u32::try_from(integer.clamp(0, i128::from(u32::MAX)))
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Spec::U64 => out.extend(
            u64::try_from(integer.clamp(0, i128::from(u64::MAX)))
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Spec::F32 => float(out, finite(value, 0.))?,
        Spec::F64 => out.extend(finite(value, 0.).to_le_bytes()),
        Spec::Text => {
            let text = if value::truth(value) {
                value::text(value)
            } else {
                String::new()
            };
            let bytes = &text.as_bytes()[..text.len().min(65535)];
            out.extend(u16::try_from(bytes.len()).unwrap_or(0).to_le_bytes());
            out.extend(bytes);
        }
        Spec::Enum(names) => {
            let direct = usize::try_from(integer)
                .ok()
                .filter(|index| *index < names.len());
            let index = direct.unwrap_or_else(|| {
                let normalized = value::text(value)
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .replace('_', "")
                    .to_lowercase();
                names
                    .iter()
                    .position(|name| name.replace('_', "").to_lowercase() == normalized)
                    .unwrap_or(0)
            });
            out.push(u8::try_from(index).unwrap_or(0));
        }
        Spec::Struct(schema) => fields(out, value, schema)?,
        Spec::StructList(schema) => {
            let items = list(value, 255);
            out.push(u8::try_from(items.len()).unwrap_or(0));
            for item in items {
                fields(out, item, schema)?;
            }
        }
        Spec::CoordList => {
            let coords: Vec<_> = list(value, 64)
                .into_iter()
                .filter_map(|item| {
                    let lat = finite(value::field(item, "latitude"), f64::NAN);
                    let lon = finite(value::field(item, "longitude"), f64::NAN);
                    (lat.is_finite() && lon.is_finite()).then_some((lat, lon))
                })
                .collect();
            out.push(u8::try_from(coords.len()).unwrap_or(0));
            if let Some((lat0, lon0)) = coords.first() {
                out.extend(lat0.to_le_bytes());
                out.extend(lon0.to_le_bytes());
                for (lat, lon) in coords.iter().skip(1) {
                    float(out, lat - lat0)?;
                    float(out, lon - lon0)?;
                }
            }
        }
        Spec::F32List
        | Spec::F32FirstList
        | Spec::I16List
        | Spec::U16CmList
        | Spec::I16CmList
        | Spec::I16MmList => pack_list(out, value, spec)?,
    }
    Ok(())
}

fn pack_list(out: &mut Vec<u8>, value: Reader<'_>, spec: &Spec) -> Result<(), Error> {
    let items = list(
        value,
        if matches!(spec, Spec::F32FirstList) {
            1
        } else {
            65535
        },
    );
    out.extend(u16::try_from(items.len()).unwrap_or(0).to_le_bytes());
    for item in items {
        match spec {
            Spec::F32List | Spec::F32FirstList => float(out, finite(item, 0.))?,
            Spec::I16List => out.extend(
                i16::try_from(value::integer(item).clamp(-32768, 32767))
                    .unwrap_or(0)
                    .to_le_bytes(),
            ),
            Spec::U16CmList => out.extend(
                (finite(item, 0.) * 100.)
                    .round_ties_even()
                    .clamp(0., 65535.)
                    .to_u16()
                    .unwrap_or(0)
                    .to_le_bytes(),
            ),
            Spec::I16CmList | Spec::I16MmList => {
                let scale = if matches!(spec, Spec::I16MmList) {
                    1000.
                } else {
                    100.
                };
                out.extend(
                    (finite(item, 0.) * scale)
                        .round_ties_even()
                        .clamp(-32768., 32767.)
                        .to_i16()
                        .unwrap_or(0)
                        .to_le_bytes(),
                );
            }
            Spec::Bool
            | Spec::I8
            | Spec::I16
            | Spec::U16
            | Spec::I32
            | Spec::U32
            | Spec::U64
            | Spec::F32
            | Spec::F64
            | Spec::Text
            | Spec::CoordList
            | Spec::Enum(_)
            | Spec::Struct(_)
            | Spec::StructList(_) => unreachable!("list match narrowed above"),
        }
    }
    Ok(())
}
