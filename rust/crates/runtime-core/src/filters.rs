//! Scalar ports of openpilot/common/filter_simple.py.
//! Callers supply finite inputs, dt > 0 and rc >= 0, as in the tested domain.

#[derive(Debug, Clone)]
pub struct FirstOrderFilter {
    x: f64,
    dt: f64,
    alpha: f64,
    initialized: bool,
}

impl FirstOrderFilter {
    pub fn new(x0: f64, rc: f64, dt: f64, initialized: bool) -> Self {
        Self {
            x: x0,
            dt,
            alpha: dt / (rc + dt),
            initialized,
        }
    }

    pub fn update_alpha(&mut self, rc: f64) {
        self.alpha = self.dt / (rc + self.dt);
    }

    pub fn update(&mut self, x: f64) -> f64 {
        if self.initialized {
            self.x = (1.0 - self.alpha) * self.x + self.alpha * x;
        } else {
            self.initialized = true;
            self.x = x;
        }
        self.x
    }

    pub fn value(&self) -> f64 {
        self.x
    }
}

#[derive(Debug, Clone)]
pub struct BounceFilter {
    position: FirstOrderFilter,
    velocity: FirstOrderFilter,
    bounce: f64,
}

impl BounceFilter {
    pub fn new(x0: f64, rc: f64, dt: f64, initialized: bool, bounce: f64) -> Self {
        Self {
            position: FirstOrderFilter::new(x0, rc, dt, initialized),
            velocity: FirstOrderFilter::new(0.0, 0.15, dt, true),
            bounce,
        }
    }

    pub fn update_alpha(&mut self, rc: f64) {
        self.position.update_alpha(rc);
    }

    pub fn update(&mut self, x: f64) -> f64 {
        self.position.update(x);
        let dt = self.position.dt;
        let scale = dt / (1.0 / 60.0);
        self.velocity.x += (x - self.position.x) * self.bounce * scale * dt;
        self.velocity.update(0.0);
        if self.velocity.x.abs() < 1e-3 {
            self.velocity.x = 0.0;
        }
        self.position.x += self.velocity.x;
        self.position.x
    }

    pub fn value(&self) -> f64 {
        self.position.x
    }
}
