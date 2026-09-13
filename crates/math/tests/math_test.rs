use math::{Complex, Mat3, Vec2, Vec3, mean, variance};

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn vec2_ops() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(3.0, 4.0);
    approx(a.dot(b), 11.0);
    approx(a.distance(b), (8.0f64).sqrt());
    let n = Vec2::new(3.0, 4.0).normalize();
    approx(n.length(), 1.0);
    assert_eq!(Vec2::ZERO.normalize(), Vec2::ZERO);
    assert_eq!(Vec2::new(0.0, 1.0).perpendicular(), Vec2::new(-1.0, 0.0));
}

#[test]
fn vec2_lerp() {
    let l = Vec2::new(0.0, 0.0).lerp(Vec2::new(10.0, 20.0), 0.5);
    assert_eq!(l, Vec2::new(5.0, 10.0));
}

#[test]
fn vec3_cross_and_transform() {
    let x = Vec3::new(1.0, 0.0, 0.0);
    let y = Vec3::new(0.0, 1.0, 0.0);
    assert_eq!(x.cross(y), Vec3::new(0.0, 0.0, 1.0));
    let rot = Mat3::rotation_z(std::f64::consts::FRAC_PI_2);
    let r = rot.transform(x);
    approx(r.x, 0.0);
    approx(r.y, 1.0);
}

#[test]
fn mat3_determinant_and_identity() {
    assert_eq!(Mat3::IDENTITY.determinant(), 1.0);
    let m = Mat3::uniform_scale(2.0);
    approx(m.determinant(), 8.0);
    assert_eq!(m.mul(&Mat3::IDENTITY), m);
    let t = Mat3::from_rows([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
    assert_eq!(t.transpose().transpose(), t);
}

#[test]
fn complex_arithmetic() {
    let a = Complex::new(1.0, 2.0);
    let b = Complex::new(3.0, -1.0);
    assert_eq!(a + b, Complex::new(4.0, 1.0));
    assert_eq!(a - b, Complex::new(-2.0, 3.0));
    assert_eq!(a * b, Complex::new(5.0, 5.0));
    approx(a.norm(), (5.0f64).sqrt());
    approx(Complex::I.arg(), std::f64::consts::FRAC_PI_2);
    assert_eq!(a * a.conjugate(), Complex::new(a.norm_sqr(), 0.0));
}

#[test]
fn complex_polar_roundtrip() {
    let c = Complex::from_polar(2.0, 0.75);
    approx(c.norm(), 2.0);
    approx(c.arg(), 0.75);
}

#[test]
fn stats_basics() {
    let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
    approx(mean(&data).unwrap(), 5.0);
    approx(variance(&data).unwrap(), 32.0 / 7.0); // sample variance (n-1)
    assert_eq!(mean(&[]), None);
    assert_eq!(variance(&[1.0]), Some(0.0));
}
