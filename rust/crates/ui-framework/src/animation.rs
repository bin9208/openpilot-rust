#[derive(Clone, Debug)]
pub struct Filter {
    pub x: f64,
    dt: f64,
    alpha: f64,
}
impl Filter {
    pub fn new(x: f64, rc: f64, fps: f64) -> Self {
        let dt = 1.0 / fps;
        Self {
            x,
            dt,
            alpha: dt / (rc + dt),
        }
    }
    pub fn update_alpha(&mut self, rc: f64) {
        self.alpha = self.dt / (rc + self.dt);
    }
    pub fn update(&mut self, x: f64) -> f64 {
        self.x = (1.0 - self.alpha) * self.x + self.alpha * x;
        self.x
    }
}
#[derive(Clone, Debug)]
pub struct Bounce {
    pub position: Filter,
    pub velocity: Filter,
    bounce: f64,
}
impl Bounce {
    pub fn new(x: f64, rc: f64, fps: f64, bounce: f64) -> Self {
        Self {
            position: Filter::new(x, rc, fps),
            velocity: Filter::new(0.0, 0.15, fps),
            bounce,
        }
    }
    pub fn update(&mut self, x: f64) -> f64 {
        self.position.update(x);
        let dt = self.position.dt;
        self.velocity.x += (x - self.position.x) * self.bounce * (dt / (1.0 / 60.0)) * dt;
        self.velocity.update(0.0);
        if self.velocity.x.abs() < 1e-3 {
            self.velocity.x = 0.0;
        }
        self.position.x += self.velocity.x;
        self.position.x
    }
}
