use crate::units::{Acceleration, Length, Time, Velocity};

pub const G_ACCEL: f64 = 9.80665;

pub fn final_velocity(v0: Velocity, a: Acceleration, t: Time) -> Velocity {
    Velocity::m_per_s(v0.as_m_per_s() + a.as_m_per_s2() * t.as_s())
}

pub fn displacement_constant_accel(v0: Velocity, a: Acceleration, t: Time) -> Length {
    let s = v0.as_m_per_s() * t.as_s() + 0.5 * a.as_m_per_s2() * t.as_s() * t.as_s();
    Length::m(s)
}

pub fn acceleration(v0: Velocity, v1: Velocity, t: Time) -> Acceleration {
    Acceleration::m_per_s2((v1.as_m_per_s() - v0.as_m_per_s()) / t.as_s())
}

pub fn time_of_flight(v0_y: Velocity, drop_height: Length) -> Time {
    // 0.5 g t^2 - v0 t - drop = 0  (up positive)
    let v = v0_y.as_m_per_s();
    let h = drop_height.as_m();
    let t = (v + (v * v + 2.0 * G_ACCEL * h).sqrt()) / G_ACCEL;
    Time::s(t)
}

pub fn projectile_range(v0: Velocity, angle_rad: f64) -> Length {
    let (vy, vx) = angle_rad.sin_cos();
    let t = time_of_flight(Velocity::m_per_s(v0.as_m_per_s() * vy), Length::m(0.0));
    Length::m(v0.as_m_per_s() * vx * t.as_s())
}
