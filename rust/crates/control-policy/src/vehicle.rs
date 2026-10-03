use crate::{math::divide, Error};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Physical {
    pub mass: f64,
    pub inertia: f64,
    pub wheelbase: f64,
    pub center_front: f64,
    pub rear_ratio: f64,
    pub stiffness_front: f64,
    pub stiffness_rear: f64,
    pub steer_ratio: f64,
}
pub struct VehicleModel {
    pub physical: Physical,
    pub c_front: f64,
    pub c_rear: f64,
    pub steer_ratio: f64,
}
impl VehicleModel {
    pub fn new(physical: Physical) -> Self {
        let c_front = physical.stiffness_front;
        let c_rear = physical.stiffness_rear;
        let steer_ratio = physical.steer_ratio;
        Self {
            physical,
            c_front,
            c_rear,
            steer_ratio,
        }
    }
    pub fn update(&mut self, stiffness: f64, ratio: f64) {
        self.c_front = stiffness * self.physical.stiffness_front;
        self.c_rear = stiffness * self.physical.stiffness_rear;
        self.steer_ratio = ratio;
    }
    fn slip(&self) -> Result<f64, Error> {
        let p = &self.physical;
        divide(
            p.mass * (self.c_front * p.center_front - self.c_rear * (p.wheelbase - p.center_front)),
            p.wheelbase.powi(2) * self.c_front * self.c_rear,
        )
    }
    pub fn factor(&self, speed: f64) -> Result<f64, Error> {
        divide(
            divide(
                1. - self.physical.rear_ratio,
                1. - self.slip()? * speed.powi(2),
            )?,
            self.physical.wheelbase,
        )
    }
    pub fn roll(&self, roll: f64, speed: f64) -> Result<f64, Error> {
        let slip = self.slip()?;
        if slip.abs() < 1e-6 {
            Ok(0.)
        } else {
            divide(9.81 * roll, 1. / slip - speed.powi(2))
        }
    }
    pub fn curvature(&self, angle: f64, speed: f64, roll: f64) -> Result<f64, Error> {
        Ok(divide(self.factor(speed)? * angle, self.steer_ratio)? + self.roll(roll, speed)?)
    }
    pub fn steer(&self, curvature: f64, speed: f64, roll: f64) -> Result<f64, Error> {
        divide(
            (curvature - self.roll(roll, speed)?) * self.steer_ratio * 1.,
            self.factor(speed)?,
        )
    }
}
