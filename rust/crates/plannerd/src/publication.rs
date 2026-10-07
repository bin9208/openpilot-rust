use crate::{number, Error};
use capnp::primitive_list;

mod lateral;
mod longitudinal;
pub use lateral::{assistance, lateral};
pub use longitudinal::{longitudinal, Metadata};

fn values(mut output: primitive_list::Builder<'_, f32>, values: &[f64]) -> Result<(), Error> {
    for (index, value) in values.iter().enumerate() {
        output.set(
            u32::try_from(index).map_err(|_| Error::Contract("publication index"))?,
            number::float32(*value)?,
        );
    }
    Ok(())
}
