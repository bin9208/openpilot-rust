use crate::{bridge::ffi, model, types::diagonal, Error};

pub struct PoseKalman {
    inner: cxx::UniquePtr<ffi::Filter>,
}
impl PoseKalman {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            inner: ffi::new_filter(
                &model::INITIAL_X,
                &diagonal::<18, 324>(model::INITIAL_P),
                &diagonal::<18, 324>(model::PROCESS_NOISE),
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
    pub fn reset_default(&mut self, time: Option<f64>) -> Result<(), Error> {
        self.reset(
            time,
            &model::INITIAL_X,
            &diagonal::<18, 324>(model::INITIAL_P),
        )
    }
    pub fn observe(
        &mut self,
        time: f64,
        kind: i32,
        values: [f64; 3],
        noise: Option<[f64; 3]>,
    ) -> Result<ffi::Estimate, Error> {
        let noise = match noise {
            Some(noise) => noise,
            None => match kind {
                4 => model::NOISE_4,
                10 => model::NOISE_10,
                13 => model::NOISE_13,
                14 => model::NOISE_14,
                _ => return Err(Error::Contract("observation kind")),
            },
        };
        Ok(self
            .inner
            .pin_mut()
            .observe(time, kind, &values, &diagonal::<3, 9>(noise))?)
    }
}
