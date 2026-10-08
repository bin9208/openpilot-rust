//! Original services/params.py QR backup codec, with explicit Params input.
mod base;
mod binary;
mod build;
mod compression;
mod digits;
mod parse;
mod schema;
mod text;
mod values;
mod zlib;

use crate::{params::Backend, Error, Value};
pub use schema::Schema;

pub struct Codec {
    schema: Option<Schema>,
    brotli: Option<crate::static_web::brotli::Brotli>,
}

impl Codec {
    pub fn new(backend: &Backend) -> Self {
        Self::with_schema(Schema::from_backend(backend).ok(), true)
    }

    pub fn with_schema(schema: Option<Schema>, brotli_available: bool) -> Self {
        Self {
            schema,
            brotli: brotli_available
                .then(crate::static_web::brotli::Brotli::load)
                .flatten(),
        }
    }

    fn required_schema(&self) -> Result<&Schema, Error> {
        self.schema
            .as_ref()
            .ok_or_else(|| error("Params/ParamKeyType not available"))
    }

    pub fn parse(&self, data: &Value) -> Result<Value, Error> {
        parse::payload(self, data)
    }

    pub fn build(&self, values: &Value) -> Result<Value, Error> {
        if let Ok(result) = self.build_version(values, 3) {
            return Ok(result);
        }
        if let Ok(result) = self.build_version(values, 4) {
            return Ok(result);
        }
        self.build_version(values, 2)
    }

    pub fn build_version(&self, values: &Value, version: u8) -> Result<Value, Error> {
        build::payload(self, values, version)
    }

    pub fn binary(&self, values: &Value, version: u8) -> Result<Vec<u8>, Error> {
        binary::build(self, values, version)
    }

    pub fn parse_binary(&self, raw: &[u8]) -> Result<Value, Error> {
        binary::parse(self, raw)
    }
}

pub fn parse(data: &Value, backend: &Backend) -> Result<Value, Error> {
    Codec::new(backend).parse(data)
}

pub fn build(values: Option<&Value>, backend: &Backend) -> Result<Value, Error> {
    let backup;
    let values = match values {
        Some(values) => values,
        None => {
            backup = backend.backup_values()?;
            &backup
        }
    };
    Codec::new(backend).build(values)
}

fn error(message: impl Into<String>) -> Error {
    Error::Source(message.into())
}
