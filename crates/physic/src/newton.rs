use crate::kinematics::G_ACCEL;
use crate::units::{Energy, Force, Length, Mass, Power, Time, Velocity};

pub fn force(mass: Mass, accel_m_per_s2: f64) -> Force {
    Force::newton(mass.as_kg() * accel_m_per_s2)
}

pub fn weight(mass: Mass) -> Force {
    Force::newton(mass.as_kg() * G_ACCEL)
}

pub fn momentum(mass: Mass, v: Velocity) -> f64 {
    mass.as_kg() * v.as_m_per_s()
}

pub fn kinetic_energy(mass: Mass, v: Velocity) -> Energy {
    let s = v.as_m_per_s();
    Energy::joule(0.5 * mass.as_kg() * s * s)
}

pub fn power(energy: Energy, t: Time) -> Power {
    Power::watt(energy.as_joule() / t.as_s())
}

pub fn gravitational_potential(mass: Mass, height: Length) -> Energy {
    Energy::joule(mass.as_kg() * G_ACCEL * height.as_m())
}
