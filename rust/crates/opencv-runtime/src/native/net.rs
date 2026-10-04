use super::{native_error, ready};
use crate::bridge::ffi;
use crate::{Error, Tensor, TensorView};
use std::{marker::PhantomData, path::Path, rc::Rc};

/// A DNN owner confined to its creating thread; no borrowed input survives forward.
pub struct DnnNet {
    net: cxx::UniquePtr<ffi::Net>,
    owner_thread: PhantomData<Rc<()>>,
}

impl DnnNet {
    pub fn load(path: &Path) -> Result<Self, Error> {
        ready()?;
        let name = path
            .to_str()
            .ok_or(Error::Contract("ONNX path must be UTF-8"))?;
        if name.as_bytes().contains(&0) {
            return Err(Error::Contract("ONNX path contains NUL"));
        }
        let net = ffi::load_onnx(name).map_err(|error| native_error("readNetFromONNX", error))?;
        Ok(Self {
            net,
            owner_thread: PhantomData,
        })
    }
    pub fn output_names(&self) -> Result<Vec<String>, Error> {
        ffi::output_names(&self.net)
            .map_err(|error| native_error("getUnconnectedOutLayersNames", error))
    }
    /// Empty output names select the original default forward() result.
    pub fn forward(
        &mut self,
        input: TensorView<'_>,
        names: &[String],
    ) -> Result<Vec<Tensor>, Error> {
        if names.iter().any(|name| name.as_bytes().contains(&0)) {
            return Err(Error::Contract("output name contains NUL"));
        }
        let outputs = ffi::forward(self.net.pin_mut(), input.values(), input.shape(), names)
            .map_err(|error| native_error("setInput/forward", error))?;
        if outputs.len() != names.len().max(1) {
            return Err(Error::Contract(
                "DNN result count differs from requested names",
            ));
        }
        outputs
            .into_iter()
            .map(|output| Tensor::from_parts(output.shape, output.values))
            .collect()
    }
}
