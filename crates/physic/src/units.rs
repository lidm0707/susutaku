#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Length(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Mass(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Time(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Velocity(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Acceleration(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Force(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Energy(f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Power(f64);

macro_rules! quantity {
    ($t:ident, $base:ident, $as_base:ident, $unit:expr) => {
        impl $t {
            pub fn $base(v: f64) -> Self {
                Self(v)
            }

            pub fn $as_base(&self) -> f64 {
                self.0
            }

            pub fn unit_name() -> &'static str {
                $unit
            }
        }
    };
}

quantity!(Length, m, as_m, "m");
quantity!(Mass, kg, as_kg, "kg");
quantity!(Time, s, as_s, "s");
quantity!(Velocity, m_per_s, as_m_per_s, "m/s");
quantity!(Acceleration, m_per_s2, as_m_per_s2, "m/s^2");
quantity!(Force, newton, as_newton, "N");
quantity!(Energy, joule, as_joule, "J");
quantity!(Power, watt, as_watt, "W");

impl Velocity {
    pub fn from_km_h(km_h: f64) -> Self {
        Self(km_h / 3.6)
    }

    pub fn as_km_h(&self) -> f64 {
        self.0 * 3.6
    }
}

impl Time {
    pub fn from_minutes(min: f64) -> Self {
        Self(min * 60.0)
    }

    pub fn from_hours(h: f64) -> Self {
        Self(h * 3600.0)
    }
}

impl Length {
    pub fn from_km(km: f64) -> Self {
        Self(km * 1000.0)
    }

    pub fn as_km(&self) -> f64 {
        self.0 / 1000.0
    }
}

impl Mass {
    pub fn from_g(g: f64) -> Self {
        Self(g / 1000.0)
    }
}

impl Energy {
    pub fn from_kwh(kwh: f64) -> Self {
        Self(kwh * 3_600_000.0)
    }
}
