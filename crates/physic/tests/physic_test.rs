use physic::{
    acceleration, displacement_constant_accel, final_velocity, force, gravitational_potential,
    kinetic_energy, momentum, power, projectile_range, time_of_flight, weight, Energy, Length,
    Mass, Time, Velocity,
};

const EPS: f64 = 1e-9;

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "{a} != {b}");
}

#[test]
fn kinematics_free_fall() {
    let v = final_velocity(
        Velocity::m_per_s(0.0),
        physic::Acceleration::m_per_s2(10.0),
        Time::s(3.0),
    );
    approx(v.as_m_per_s(), 30.0);
    let d = displacement_constant_accel(
        Velocity::m_per_s(0.0),
        physic::Acceleration::m_per_s2(10.0),
        Time::s(3.0),
    );
    approx(d.as_m(), 45.0);
}

#[test]
fn acceleration_definition() {
    let a = acceleration(
        Velocity::m_per_s(0.0),
        Velocity::m_per_s(20.0),
        Time::s(4.0),
    );
    approx(a.as_m_per_s2(), 5.0);
}

#[test]
fn drop_time() {
    let t = time_of_flight(Velocity::m_per_s(0.0), Length::m(19.6133));
    approx(t.as_s(), 2.0);
}

#[test]
fn projectile_range_45deg() {
    let r = projectile_range(Velocity::m_per_s(10.0), std::f64::consts::FRAC_PI_4);
    assert!(r.as_m() > 0.0);
    let r30 = projectile_range(Velocity::m_per_s(10.0), std::f64::consts::PI / 6.0);
    assert!(r30.as_m() < r.as_m());
}

#[test]
fn newton_basics() {
    let m = Mass::kg(2.0);
    approx(weight(m).as_newton(), 2.0 * 9.80665);
    approx(force(m, 5.0).as_newton(), 10.0);
    approx(momentum(m, Velocity::m_per_s(3.0)), 6.0);
    approx(kinetic_energy(m, Velocity::m_per_s(3.0)).as_joule(), 9.0);
    approx(
        gravitational_potential(m, Length::m(10.0)).as_joule(),
        2.0 * 9.80665 * 10.0,
    );
    approx(power(Energy::joule(100.0), Time::s(4.0)).as_watt(), 25.0);
}

#[test]
fn unit_conversions() {
    approx(Velocity::from_km_h(36.0).as_m_per_s(), 10.0);
    approx(Velocity::m_per_s(10.0).as_km_h(), 36.0);
    approx(Time::from_minutes(2.0).as_s(), 120.0);
    approx(Time::from_hours(1.0).as_s(), 3600.0);
    approx(Length::from_km(1.0).as_m(), 1000.0);
    approx(Length::m(1500.0).as_km(), 1.5);
    approx(Mass::from_g(500.0).as_kg(), 0.5);
    approx(Energy::from_kwh(1.0).as_joule(), 3_600_000.0);
    assert_eq!(Energy::unit_name(), "J");
}
