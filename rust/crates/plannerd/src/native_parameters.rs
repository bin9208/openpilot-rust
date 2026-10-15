use crate::{Error, parameters::Parameters};
use openpilot_params::Params;

pub struct RuntimeParameters(pub Params);

impl RuntimeParameters {
    fn raw(&self, key: &str) -> Result<Vec<u8>, Error> {
        match self.0.get(key) {
            Ok(value) => Ok(value.unwrap_or_default()),
            Err(openpilot_params::Error::Io(_)) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn car_params(&self) -> Result<Vec<u8>, Error> {
        self.raw("CarParams")
    }
}

pub fn integer(bytes: &[u8]) -> Result<i32, Error> {
    Ok(openpilot_beepd::integer(bytes)?)
}
pub fn float(bytes: &[u8]) -> Result<f64, Error> {
    Ok(openpilot_calibrationd::parameters::parse_float(bytes)?)
}

impl Parameters for RuntimeParameters {
    fn stopping_speed(&mut self) -> Result<f64, Error> {
        let raw = self.raw("VEgoStopping")?;
        let value = openpilot_control_policy::stopping::speed(float(&raw).unwrap_or(f64::NAN));
        if let Some(repair) = value.repair {
            stopping_repairs().try_send((self.0.clone(), repair)).ok();
        }
        Ok(value.meters_per_second)
    }
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        integer(&self.raw(key)?)
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        float(&self.raw(key)?)
    }
}

// A bounded persistent worker keeps Params fsync and external-write repairs off
// the model cycle. A busy queue is retried on the next cycle while the bound is
// already enforced in memory.
fn stopping_repairs() -> &'static std::sync::mpsc::SyncSender<(Params, i32)> {
    static WRITES: std::sync::OnceLock<std::sync::mpsc::SyncSender<(Params, i32)>> =
        std::sync::OnceLock::new();
    WRITES.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<(Params, i32)>(1);
        std::thread::Builder::new()
            .name("planner-params".into())
            .spawn(move || {
                while let Ok((params, value)) = receiver.recv() {
                    let _ = params.put("VEgoStopping", value.to_string().as_bytes());
                }
            })
            .expect("planner Params repair worker");
        sender
    })
}
