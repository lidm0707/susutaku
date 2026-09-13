use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };
    pub const ONE: Self = Self { re: 1.0, im: 0.0 };
    pub const I: Self = Self { re: 0.0, im: 1.0 };

    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    pub fn from_polar(mag: f64, angle_rad: f64) -> Self {
        Self::new(mag * angle_rad.cos(), mag * angle_rad.sin())
    }

    pub fn norm(self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }

    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    pub fn conjugate(self) -> Self {
        Self::new(self.re, -self.im)
    }

    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    pub fn scale(self, factor: f64) -> Self {
        Self::new(self.re * factor, self.im * factor)
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl Sub for Complex {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl Mul for Complex {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

impl Mul<f64> for Complex {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        self.scale(rhs)
    }
}

impl Div for Complex {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let d = rhs.norm_sqr();
        Self::new(
            (self.re * rhs.re + self.im * rhs.im) / d,
            (self.im * rhs.re - self.re * rhs.im) / d,
        )
    }
}

impl Div<f64> for Complex {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self::new(self.re / rhs, self.im / rhs)
    }
}

impl Neg for Complex {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}
