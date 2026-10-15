use crate::Error;

pub trait Parameters {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error>;
    fn float(&mut self, key: &'static str) -> Result<f64, Error>;
    fn stopping_speed(&mut self) -> Result<f64, Error> {
        let stored = self.float("VEgoStopping").unwrap_or(f64::NAN);
        Ok(openpilot_control_policy::stopping::speed(stored).meters_per_second)
    }
}
