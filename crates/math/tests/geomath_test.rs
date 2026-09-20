#![allow(clippy::unwrap_used)] // tests: unwrap is the assertion tool

use math::geomath::eval;
use math::{Vec2, geomath};

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn triangle_and_heron() {
    approx(geomath::triangle_area(6.0, 4.0), 12.0);
    approx(geomath::heron(3.0, 4.0, 5.0).unwrap(), 6.0);
    assert!(geomath::heron(1.0, 2.0, 10.0).is_err());
}

#[test]
fn circle() {
    approx(geomath::circle_area(1.0), std::f64::consts::PI);
    approx(
        geomath::circle_circumference(2.0),
        4.0 * std::f64::consts::PI,
    );
}

#[test]
fn polygon_shoelace() {
    let square = [
        Vec2::new(0.0, 0.0),
        Vec2::new(4.0, 0.0),
        Vec2::new(4.0, 4.0),
        Vec2::new(0.0, 4.0),
    ];
    approx(geomath::polygon_area(&square), 16.0);
    assert_eq!(geomath::polygon_area(&square[..2]), 0.0);
}

#[test]
fn pythag_and_distance() {
    approx(geomath::pythag(3.0, 4.0), 5.0);
    approx(
        geomath::eval("distance 0 0 3 4").unwrap().parse().unwrap(),
        5.0,
    );
}

#[test]
fn eval_ops() {
    assert_eq!(eval("triangle 6 4").unwrap(), "12");
    assert_eq!(eval("pythag 3 4").unwrap(), "5");
    assert!(eval("heron 3 4 5").unwrap().starts_with('6'));
    assert!(eval("circle 1").unwrap().contains("area"));
    assert!(eval("polygon 0,0 4,0 4,4 0,4").unwrap().starts_with("16"));
    assert!(eval("haversine 0 0 0 1").unwrap().ends_with("km"));
    assert!(eval("nope 1").is_err());
    assert!(eval("").is_err());
}
