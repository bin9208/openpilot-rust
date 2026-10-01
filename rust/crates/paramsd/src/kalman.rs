use crate::{bridge::ffi, model, types::diagonal, Error};

pub struct CarKalman {
    inner: cxx::UniquePtr<ffi::Filter>,
}
impl CarKalman {
    pub fn new(globals: &[f64; 6]) -> Result<Self, Error> {
        Ok(Self {
            inner: ffi::new_filter(
                &model::INITIAL_X,
                &diagonal(&model::INITIAL_P),
                &diagonal(&model::PROCESS_NOISE),
                globals,
            )?,
        })
    }
    pub fn snapshot(&self) -> Result<ffi::Snapshot, Error> {
        Ok(self
            .inner
            .as_ref()
            .ok_or(Error::Contract("null filter"))?
            .snapshot())
    }
    pub fn reset(&mut self, time: Option<f64>, x: &[f64], covariance: &[f64]) -> Result<(), Error> {
        Ok(self
            .inner
            .pin_mut()
            .reset(x, covariance, time.unwrap_or(f64::NAN))?)
    }
    pub fn pause(&mut self, time: f64) {
        self.inner.pin_mut().pause(time);
    }
    pub fn observe(
        &mut self,
        time: f64,
        kind: i32,
        value: f64,
        noise: Option<f64>,
    ) -> Result<ffi::Estimate, Error> {
        let noise = match noise {
            Some(value) => value,
            None => match kind {
                26 => model::NOISE_26,
                27 => model::NOISE_27,
                28 => model::NOISE_28,
                29 => model::NOISE_29,
                30 => model::NOISE_30,
                31 => model::NOISE_31,
                _ => return Err(Error::Contract("observation requires covariance")),
            },
        };
        Ok(self
            .inner
            .pin_mut()
            .observe(time, kind, &[value], &[noise])?)
    }
}
