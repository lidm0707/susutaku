const TWO_PI: f32 = std::f32::consts::TAU;

#[derive(Clone, Copy)]
pub struct Animation {
    pub speed: f32,
    pub t: f32,
}

impl Animation {
    pub const fn new(speed: f32) -> Self {
        Self { speed, t: 0.0 }
    }

    pub fn tick(&mut self, dt: f32) {
        self.t = (self.t + dt * self.speed) % TWO_PI;
    }

    pub fn phase(&self) -> f32 {
        self.t
    }
}
