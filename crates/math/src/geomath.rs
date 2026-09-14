use crate::Vec2;
use std::f64::consts::PI;

const HALF: f64 = 0.5;
const EARTH_RADIUS_KM: f64 = 6371.0;
const POLYGON_MIN_SIDES: usize = 3;
const OP_DISTANCE: &str = "distance";
const OP_TRIANGLE: &str = "triangle";
const OP_HERON: &str = "heron";
const OP_CIRCLE: &str = "circle";
const OP_POLYGON: &str = "polygon";
const OP_PYTHAG: &str = "pythag";
const OP_HAVERSINE: &str = "haversine";

pub fn triangle_area(base: f64, height: f64) -> f64 {
    HALF * base * height
}

pub fn heron(a: f64, b: f64, c: f64) -> Result<f64, String> {
    let s = (a + b + c) / 2.0;
    let squared = s * (s - a) * (s - b) * (s - c);
    if squared <= 0.0 {
        return Err(format!("invalid triangle sides {a} {b} {c}"));
    }
    Ok(squared.sqrt())
}

pub fn circle_area(radius: f64) -> f64 {
    PI * radius * radius
}

pub fn circle_circumference(radius: f64) -> f64 {
    2.0 * PI * radius
}

pub fn polygon_area(points: &[Vec2]) -> f64 {
    let n = points.len();
    if n < POLYGON_MIN_SIDES {
        return 0.0;
    }
    let mut doubled = 0.0;
    for (i, a) in points.iter().enumerate() {
        let b = points[(i + 1) % n];
        doubled += a.x * b.y - b.x * a.y;
    }
    (doubled / 2.0).abs()
}

pub fn pythag(a: f64, b: f64) -> f64 {
    a.hypot(b)
}

pub fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (lat1, lat2) = (lat1.to_radians(), lat2.to_radians());
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

enum Op {
    Distance,
    Triangle,
    Heron,
    Circle,
    Polygon,
    Pythag,
    Haversine,
}

fn parse_op(name: &str) -> Option<Op> {
    match name {
        OP_DISTANCE => Some(Op::Distance),
        OP_TRIANGLE => Some(Op::Triangle),
        OP_HERON => Some(Op::Heron),
        OP_CIRCLE => Some(Op::Circle),
        OP_POLYGON => Some(Op::Polygon),
        OP_PYTHAG => Some(Op::Pythag),
        OP_HAVERSINE => Some(Op::Haversine),
        _ => None,
    }
}

fn parse_num(token: &str) -> Result<f64, String> {
    token
        .trim()
        .parse()
        .map_err(|_| format!("expected number, got {token:?}"))
}

fn parse_point(token: &str) -> Result<Vec2, String> {
    let (x, y) = token
        .split_once(',')
        .ok_or_else(|| format!("expected x,y point, got {token:?}"))?;
    Ok(Vec2::new(parse_num(x)?, parse_num(y)?))
}

fn parse_nums(tokens: &[&str]) -> Result<Vec<f64>, String> {
    tokens.iter().map(|t| parse_num(t)).collect()
}

fn parse_points(tokens: &[&str]) -> Result<Vec<Vec2>, String> {
    tokens.iter().map(|t| parse_point(t)).collect()
}

fn eval_distance(tokens: &[&str]) -> Result<String, String> {
    let d = if tokens.iter().all(|t| t.contains(',')) {
        let pts = parse_points(tokens)?;
        if pts.len() != 2 {
            return Err("distance needs 2 points".into());
        }
        pts[0].distance(pts[1])
    } else {
        let n = parse_nums(tokens)?;
        if n.len() != 4 {
            return Err("distance needs x1 y1 x2 y2".into());
        }
        Vec2::new(n[0], n[1]).distance(Vec2::new(n[2], n[3]))
    };
    Ok(format!("{d}"))
}

fn eval_polygon(tokens: &[&str]) -> Result<String, String> {
    let pts = parse_points(tokens)?;
    if pts.len() < POLYGON_MIN_SIDES {
        return Err(format!("polygon needs at least {POLYGON_MIN_SIDES} points"));
    }
    Ok(format!("{}", polygon_area(&pts)))
}

pub fn eval(arg: &str) -> Result<String, String> {
    let mut parts = arg.split_whitespace();
    let name = parts.next().ok_or("empty math expression")?;
    let tokens: Vec<&str> = parts.collect();
    match parse_op(name) {
        Some(Op::Distance) => eval_distance(&tokens),
        Some(Op::Triangle) => match parse_nums(&tokens)?.as_slice() {
            [base, height] => Ok(format!("{}", triangle_area(*base, *height))),
            _ => Err("triangle needs base height".into()),
        },
        Some(Op::Heron) => match parse_nums(&tokens)?.as_slice() {
            [a, b, c] => Ok(format!("{}", heron(*a, *b, *c)?)),
            _ => Err("heron needs a b c".into()),
        },
        Some(Op::Circle) => match parse_nums(&tokens)?.as_slice() {
            [r] => Ok(format!(
                "area {} circumference {}",
                circle_area(*r),
                circle_circumference(*r)
            )),
            _ => Err("circle needs radius".into()),
        },
        Some(Op::Polygon) => eval_polygon(&tokens),
        Some(Op::Pythag) => match parse_nums(&tokens)?.as_slice() {
            [a, b] => Ok(format!("{}", pythag(*a, *b))),
            _ => Err("pythag needs a b".into()),
        },
        Some(Op::Haversine) => match parse_nums(&tokens)?.as_slice() {
            [lat1, lon1, lat2, lon2] => Ok(format!("{} km", haversine(*lat1, *lon1, *lat2, *lon2))),
            _ => Err("haversine needs lat1 lon1 lat2 lon2".into()),
        },
        None => Err(format!(
            "unknown geomath op {name:?} (use {OP_DISTANCE}|{OP_TRIANGLE}|{OP_HERON}|{OP_CIRCLE}|{OP_POLYGON}|{OP_PYTHAG}|{OP_HAVERSINE})"
        )),
    }
}
