use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    ops::{Add, Div, Mul, Sub},
};

use ab_glyph::{Font, FontRef, Outline, OutlineCurve, Point, Rect};
use bytemuck::Zeroable;

pub const FONT_DATA: &[u8] = include_bytes!("test_font.ttf");

#[derive(Debug)]
pub struct FontSDF {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub locations: HashMap<char, (f64, Rect)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Vector {
    x: f64,
    y: f64,
}

impl Add for Vector {
    type Output = Vector;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: rhs.x + self.x,
            y: rhs.y + self.y,
        }
    }
}

impl Add<f64> for Vector {
    type Output = Vector;

    fn add(self, rhs: f64) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

const EPSILON: f64 = 1e-15;

impl<F: Into<f64>> Mul<F> for Vector {
    type Output = Self;

    fn mul(self, rhs: F) -> Self::Output {
        let v = rhs.into();
        Self {
            x: self.x * v,
            y: self.y * v,
        }
    }
}

impl<F: Into<f64>> Div<F> for Vector {
    type Output = Self;

    fn div(self, rhs: F) -> Self::Output {
        let rhs = rhs.into();
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl Sub for Vector {
    type Output = Vector;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Vector {
    fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }

    fn sq_dist(self, rhs: Self) -> f64 {
        (rhs.x - self.x).powi(2) + (rhs.y - self.y).powi(2)
    }

    fn mag(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn norm(self) -> Self {
        let mag = self.mag();
        Self {
            x: self.x / mag,
            y: self.y / mag,
        }
    }

    fn cross(&self, s0: Vector) -> f64 {
        self.x * s0.y - self.y * s0.x
    }
}

impl From<Point> for Vector {
    fn from(value: Point) -> Self {
        Self {
            x: value.x as f64,
            y: value.y as f64,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Segment {
    LINE(Line),
    BEZ2(Bez2),
    BEZ3(Bez3),
}

impl Div<f64> for Segment {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        match self {
            Segment::LINE(line) => Self::LINE(line / rhs),
            Segment::BEZ2(bez2) => Self::BEZ2(bez2 / rhs),
            Segment::BEZ3(bez3) => Self::BEZ3(bez3 / rhs),
        }
    }
}

impl Add<f64> for Segment {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        match self {
            Segment::LINE(line) => Self::LINE(line + rhs),
            Segment::BEZ2(bez2) => Self::BEZ2(bez2 + rhs),
            Segment::BEZ3(bez3) => Self::BEZ3(bez3 + rhs),
        }
    }
}

impl Segment {
    fn end(&self) -> Vector {
        match self {
            Segment::LINE(line) => line.1.clone(),
            Segment::BEZ2(bez2) => bez2.2.clone(),
            Segment::BEZ3(bez3) => bez3.3.clone(),
        }
    }

    fn start(&self) -> Vector {
        match self {
            Segment::LINE(line) => line.0.clone(),
            Segment::BEZ2(bez2) => bez2.0.clone(),
            Segment::BEZ3(bez3) => bez3.0.clone(),
        }
    }

    fn start_direction(&self) -> Vector {
        match self {
            Segment::LINE(line) => line.1 - line.0,
            Segment::BEZ2(bez2) => bez2.1 - bez2.0,
            Segment::BEZ3(bez3) => bez3.1 - bez3.0,
        }
    }

    fn end_direction(&self) -> Vector {
        match self {
            Segment::LINE(line) => line.1 - line.0,
            Segment::BEZ2(bez2) => bez2.2 - bez2.1,
            Segment::BEZ3(bez3) => bez3.3 - bez3.2,
        }
    }

    fn sdistance(&self, p: Vector) -> f64 {
        match self {
            Segment::LINE(line) => line.sdistance(p),
            Segment::BEZ2(bez2) => bez2.sdistance(p),
            Segment::BEZ3(bez3) => bez3.sdistance(p),
        }
    }

    fn pseudo_sdistance(&self, p: Vector) -> f64 {
        match self {
            Segment::LINE(line) => line.pseudo_sdistance(p),
            Segment::BEZ2(bez2) => bez2.pseudo_sdistance(p),
            Segment::BEZ3(bez3) => bez3.pseudo_sdistance(p),
        }
    }
}

impl Add<Vector> for Segment {
    type Output = Self;

    fn add(self, rhs: Vector) -> Self::Output {
        match self {
            Self::LINE(line) => Self::LINE(Line(line.0 + rhs, line.1 + rhs)),
            Self::BEZ2(bez2) => Self::BEZ2(Bez2(bez2.0 + rhs, bez2.1 + rhs, bez2.2 + rhs)),
            Self::BEZ3(bez3) => {
                Self::BEZ3(Bez3(bez3.0 + rhs, bez3.1 + rhs, bez3.2 + rhs, bez3.3 + rhs))
            }
        }
    }
}

impl Mul<f64> for Segment {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        match self {
            Self::LINE(line) => Self::LINE(Line(line.0 * rhs, line.1 * rhs)),
            Self::BEZ2(bez2) => Self::BEZ2(Bez2(bez2.0 * rhs, bez2.1 * rhs, bez2.2 * rhs)),
            Self::BEZ3(bez3) => {
                Self::BEZ3(Bez3(bez3.0 * rhs, bez3.1 * rhs, bez3.2 * rhs, bez3.3 * rhs))
            }
        }
    }
}

impl From<OutlineCurve> for Segment {
    fn from(value: OutlineCurve) -> Self {
        match value {
            OutlineCurve::Line(point, point1) => Self::LINE(Line(point.into(), point1.into())),
            OutlineCurve::Quad(point, point1, point2) => {
                Self::BEZ2(Bez2(point.into(), point1.into(), point2.into()))
            }
            OutlineCurve::Cubic(point, point1, point2, point3) => Self::BEZ3(Bez3(
                point.into(),
                point1.into(),
                point2.into(),
                point3.into(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Line(Vector, Vector);

impl Div<f64> for Line {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs)
    }
}

impl Add<f64> for Line {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs)
    }
}

impl Line {
    fn sdistance(&self, p: Vector) -> f64 {
        if self.1 == self.0 {
            return self.1.sq_dist(p).sqrt();
        }
        let t = ((p - self.0).dot(self.1 - self.0) / (self.1 - self.0).dot(self.1 - self.0))
            .clamp(0., 1.);
        let p_prime = self.0 * (1. - t) + self.1 * t;
        p_prime
            .sq_dist(p)
            .sqrt()
            .copysign((self.1 - self.0).cross(p - p_prime))
    }

    fn pseudo_sdistance(&self, p: Vector) -> f64 {
        if self.1 == self.0 {
            return self.1.sq_dist(p).sqrt();
        }
        let t = (p - self.0).dot(self.1 - self.0) / (self.1 - self.0).dot(self.1 - self.0);
        let p_prime = self.0 * (1. - t) + self.1 * t;
        p_prime
            .sq_dist(p)
            .sqrt()
            .copysign((self.1 - self.0).cross(p - p_prime))
    }
}

#[derive(Clone, Copy, Debug)]
struct Bez2(Vector, Vector, Vector);

impl Div<f64> for Bez2 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs, self.2 / rhs)
    }
}

impl Add<f64> for Bez2 {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs, self.2 + rhs)
    }
}

impl Bez2 {
    fn sdistance(&self, p: Vector) -> f64 {
        let p0 = p - self.0;
        let p1 = self.1 - self.0;
        let p2 = self.2 - self.1 * 2. + self.0;

        let k0 = p2.dot(p2);
        let k1 = p1.dot(p2) * 3.;
        let k2 = 2. * p1.dot(p1) - p2.dot(p0);
        let k3 = p1.dot(p0) * -1.;

        let roots = solve_cubic(k0, k1, k2, k3).unwrap();
        let dist = roots
            .into_iter()
            .filter(|r| *r > 0. && *r < 1.)
            .chain([0., 1.].into_iter())
            .map(|r| {
                let p_prime = p2 * r * r + p1 * 2. * r + self.0;

                p_prime
                    .sq_dist(p)
                    .sqrt()
                    .copysign((p2 * 2. * r + p1 * 2.).cross(p - p_prime))
            })
            .min_by(|x, y| x.abs().total_cmp(&y.abs()));
        dist.unwrap()
    }

    fn pseudo_sdistance(&self, p: Vector) -> f64 {
        let p0 = p - self.0;
        let p1 = self.1 - self.0;
        let p2 = self.2 - self.1 * 2. + self.0;

        let k0 = p2.dot(p2);
        let k1 = p1.dot(p2) * 3.;
        let k2 = 2. * p1.dot(p1) - p2.dot(p0);
        let k3 = p1.dot(p0) * -1.;

        let roots = solve_cubic(k0, k1, k2, k3).unwrap();

        let min = roots
            .into_iter()
            .map(|r| {
                let p_prime = p2 * r * r + p1 * 2. * r + self.0;

                let dist = p_prime
                    .sq_dist(p)
                    .sqrt()
                    .copysign((p2 * 2. * r + p1 * 2.).cross(p - p_prime));
                dist
            })
            .min_by(|x, y| x.abs().total_cmp(&y.abs()));
        min.unwrap()
    }
}

/*
fn solve_cubic_normed(mut a: f64, b: f64, c: f64) -> Vec<f64> {
    let a2 = a * a;
    let mut q = 1. / 9. * (a2 - 3. * b);
    let r = 1. / 54. * (a * (2. * a2 - 9. * b) + 27. * c);
    let r2 = r * r;
    let q3 = q * q * q;
    a *= 1. / 3.;
    if r2 < q3 {
        let mut t = r / q3.sqrt();
        if t < -1. {
            t = -1.
        };
        if t > 1. {
            t = 1.
        };
        t = t.acos();
        q = -2. * q.sqrt();
        let x0 = q * (1. / 3. * t).cos() - a;
        let x1 = q * (1. / 3. * (t + 2. * std::f64::consts::PI)).cos() - a;
        let x2 = q * (1. / 3. * (t - 2. * std::f64::consts::PI)).cos() - a;
        return vec![x0, x1, x2];
    } else {
        let u = (if r < 0. { 1. } else { -1. }) * ((r).abs() + (r2 - q3).sqrt()).powf(1. / 3.);
        let v = if u == 0. { 0. } else { q / u };
        let x0 = (u + v) - a;
        if u == v || (u - v).abs() < 1e-12 * (u + v).abs() {
            let x1 = -0.5 * (u + v) - a;
            return vec![x0, x1];
        }
        return vec![x0];
    }
}
*/

#[derive(Debug)]
struct Clash {
    x: usize,
    y: usize,
}

const EDGE_THRESHOLD: f32 = 0.02;
const RANGE: f32 = 0.5;

fn pixel_clash(a: [f32; 3], b: [f32; 3], threshold: f32) -> bool {
    let aIn = (a[0] > 0.5) as usize + (a[1] > 0.5) as usize + ((a[2] > 0.5) as usize) >= 2;
    let bIn = (b[0] > 0.5) as usize + (b[1] > 0.5) as usize + (b[2] > 0.5) as usize >= 2;
    if aIn != bIn {
        return false;
    };
    if (a[0] > 0.5 && a[1] > 0.5 && a[2] > 0.5)
        || (a[0] < 0.5 && a[1] < 0.5 && a[2] < 0.5)
        || (b[0] > 0.5 && b[1] > 0.5 && b[2] > 0.5)
        || (b[0] < 0.5 && b[1] < 0.5 && b[2] < 0.5)
    {
        return false;
    }
    let (aa, ba, (ab, ac, bb, bc)) = if (a[0] > 0.5) != (b[0] > 0.5) && (a[0] < 0.5) != (b[0] < 0.5)
    {
        (
            a[0],
            b[0],
            if (a[1] > 0.5) != (b[1] > 0.5) && (a[1] < 0.5) != (b[1] < 0.5) {
                (a[1], a[2], b[1], b[2])
            } else if (a[2] > 0.5) != (b[2] > 0.5) && (a[2] < 0.5) != (b[2] < 0.5) {
                (a[2], a[1], b[2], b[1])
            } else {
                return false;
            },
        )
    } else if (a[1] > 0.5) != (b[1] > 0.5)
        && (a[1] < 0.5) != (b[1] < 0.5)
        && (a[2] > 0.5) != (b[2] > 0.5)
        && (a[2] < 0.5) != (b[2] < 0.5)
    {
        (a[1], b[1], (a[2], a[0], b[2], b[0]))
    } else {
        return false;
    };
    return ((aa - ba).abs() >= threshold)
        && ((ab - bb).abs() >= threshold)
        && (ac - 0.5).abs() >= (bc - 0.5).abs();
}

fn solve_cubic(a: f64, b: f64, c: f64, d: f64) -> Option<Vec<f64>> {
    if a.abs() < EPSILON {
        return solve_quadratic(b, c, d);
    }
    return solve_cubic_normed(b / a, c / a, d / a);
}

fn solve_quadratic(a: f64, b: f64, c: f64) -> Option<Vec<f64>> {
    if a.abs() < EPSILON {
        if b.abs() < EPSILON {
            // Case: 0x + 0 = 0 -> Infinite solutions (identity)
            // Or  : 0x + c = 0 (c!=0) -> No solutions
            return if c.abs() < EPSILON {
                None
            } else {
                Some(Vec::new())
            };
        }
        return Some(vec![-c / b]);
    }

    let dscr = b * b - 4.0 * a * c;

    if dscr < -EPSILON {
        // No real roots
        Some(Vec::new())
    } else if dscr.abs() < EPSILON {
        // One real root (or two very close roots)
        Some(vec![-b / (2.0 * a)])
    } else {
        // Two distinct real roots (using the stable quadratic formula)
        let sqrt_dscr = dscr.sqrt();
        // The sign of `b` is used to avoid subtraction of nearly equal numbers.
        let x1 = (-b - b.signum() * sqrt_dscr) / (2.0 * a);
        let x2 = c / (a * x1);
        Some(vec![x1, x2])
    }
}

/// Solves a normed cubic equation x^3 + ax^2 + bx + c = 0 for real roots.
fn solve_cubic_normed(a: f64, b: f64, c: f64) -> Option<Vec<f64>> {
    // Transform to depressed cubic t^3 + pt + q = 0 with x = t - a/3
    let p = b - a * a / 3.0;
    let q = c + (2.0 * a * a * a - 9.0 * a * b) / 27.0;
    let offset = -a / 3.0;

    // The discriminant here is based on Cardano's formula intermediates.
    // The sign is opposite to the standard definition of the cubic discriminant.
    let d = (q / 2.0).powi(2) + (p / 3.0).powi(3);

    if d.abs() < EPSILON {
        // Multiple roots case (d is zero)
        if p.abs() < EPSILON {
            // Triple root at t=0, since p and q are zero.
            return Some(vec![offset, offset, offset]);
        }
        // One single root and one double root
        let root_val = (-q / 2.0).cbrt();
        let single_root = 2.0 * root_val + offset;
        let double_root = -root_val + offset;
        return Some(vec![single_root, double_root, double_root]);
    } else if d > 0.0 {
        // One real root (Cardano's formula)
        let sqrt_d = d.sqrt();
        let u = (-q / 2.0 + sqrt_d).cbrt();
        let v = (-q / 2.0 - sqrt_d).cbrt();
        return Some(vec![u + v + offset]);
    } else {
        // d < 0.0
        // Three distinct real roots (trigonometric solution)
        let r = (-p / 3.0).sqrt();
        let phi = (-q / (2.0 * r.powi(3))).acos();

        let root1 = 2.0 * r * (phi / 3.0).cos() + offset;
        let root2 = 2.0 * r * ((phi + 2.0 * std::f64::consts::PI) / 3.0).cos() + offset;
        let root3 = 2.0 * r * ((phi + 4.0 * std::f64::consts::PI) / 3.0).cos() + offset;
        return Some(vec![root1, root2, root3]);
    }
}

#[derive(Clone, Copy, Debug)]
struct Bez3(Vector, Vector, Vector, Vector);

impl Div<f64> for Bez3 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs, self.2 / rhs, self.3 / rhs)
    }
}

impl Add<f64> for Bez3 {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs, self.2 + rhs, self.3 + rhs)
    }
}

impl Bez3 {
    fn sdistance(&self, p: Vector) -> f64 {
        let p0 = p - self.0;
        let p1 = self.1 - self.0;
        let p2 = self.2 - self.1 * 2. + self.0;
        let p3 = self.3 - self.2 * 3. + self.1 * 3. + self.0;

        let k0 = p3.dot(p3);
        let k1 = p2.dot(p3) * 5.;
        let k2 = (p1.dot(p3) + p2.dot(p2) * 6.) * 4.;
        let k3 = p1.dot(p2) * 9. - p2.dot(p0);
        let k4 = p1.dot(p1) * 3. - p2.dot(p0) * 2.;
        let k5 = p1.dot(p0) * -1.;
        let mut roots = match solve_quintic(k0, k1, k2, k3, k4, k5) {
            Some(r) => r,
            None => return p.sq_dist(self.1),
        }
        .to_vec();
        roots.push(0.);
        roots.push(1.);
        roots.into_iter().fold(std::f64::MAX, |min_dist, r| {
            let r = r.clamp(0., 1.);
            let p_prime = p3 * r * r * r + p2 * 3. * r * r + p1 * 3. * r + self.0;
            let dist = p_prime.sq_dist(p).sqrt();
            if dist < min_dist.abs() {
                dist * (p3 * 3. * r * r + p2 * 6. * r + p1 * 3.)
                    .cross(p - p_prime)
                    .signum()
            } else {
                min_dist
            }
        })
    }

    fn pseudo_sdistance(&self, p: Vector) -> f64 {
        let p0 = p - self.0;
        let p1 = self.1 - self.0;
        let p2 = self.2 - self.1 * 2. + self.0;
        let p3 = self.3 - self.2 * 3. + self.1 * 3. + self.0;

        let k0 = p3.dot(p3);
        let k1 = p2.dot(p3) * 5.;
        let k2 = (p1.dot(p3) + p2.dot(p2) * 6.) * 4.;
        let k3 = p1.dot(p2) * 9. - p2.dot(p0);
        let k4 = p1.dot(p1) * 3. - p2.dot(p0) * 2.;
        let k5 = p1.dot(p0) * -1.;
        let roots = match solve_quintic(k0, k1, k2, k3, k4, k5) {
            Some(r) => r,
            None => return p.sq_dist(self.1),
        }
        .to_vec();
        roots.into_iter().fold(std::f64::MAX, |min_dist, r| {
            let p_prime = p3 * r * r * r + p2 * 3. * r * r + p1 * 3. * r + self.0;
            let dist = p_prime.sq_dist(p).sqrt();
            if dist < min_dist.abs() {
                dist * (p3 * 3. * r * r + p2 * 6. * r + p1 * 3.)
                    .cross(p - p_prime)
                    .signum()
            } else {
                min_dist
            }
        })
    }
}

fn eval_quintic(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64, t: f64) -> f64 {
    t.powi(5) * a + t.powi(4) * b + t.powi(3) * c + t.powi(2) * d + t * e + f
}

fn solve_quintic(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Option<Vec<f64>> {
    // 1. Find roots of the derivative to get critical points
    let d_a = a * 5.;
    let d_b = b * 4.;
    let d_c = c * 3.;
    let d_d = d * 2.;
    let d_e = e;

    let mut crit_points = match solve_quartic(d_a, d_b, d_c, d_d, d_e) {
        Some(r) => r,
        None => return None,
    }
    .to_vec();
    crit_points.sort_floats();

    let mut search_points = Vec::with_capacity(6);
    // 2. Create intervals to search for roots

    // Start with a point far to the left
    let mut m: f64 = 1.0;
    m = m.max(a.abs());
    m = m.max(b.abs());
    m = m.max(c.abs());
    m = m.max(d.abs());
    m = m.max(e.abs());
    m = m.max(f.abs());

    let bound = 1. + m / a.abs();
    search_points.push(-bound);

    for crit_point in crit_points {
        search_points.push(crit_point);
    }
    search_points.push(bound);

    let mut roots = Vec::with_capacity(5);
    for search_interval in search_points.windows(2) {
        let ya = eval_quintic(a, b, c, d, e, f, search_interval[0]);
        let yb = eval_quintic(a, b, c, d, e, f, search_interval[1]);
        if ya.abs() < EPSILON {
            roots.push(search_interval[0]);
        } else if ya * yb < 0. {
            let root: f64 = find_root_bisection_quintic(
                a,
                b,
                c,
                d,
                e,
                f,
                search_interval[0],
                search_interval[1],
            );
            if !root.is_nan() {
                roots.push(root);
            }
        }
    }
    let last_y = eval_quintic(
        a,
        b,
        c,
        d,
        e,
        f,
        *search_points
            .last()
            .expect("To have atleast 1 search point"),
    );
    if last_y.abs() < EPSILON {
        roots.push(
            *search_points
                .last()
                .expect("To have atleast 1 search point"),
        );
    }

    // Remove duplicates
    if roots.len() > 1 {
        roots.sort_floats();
        roots.dedup_by(|a, b| (*a - *b).abs() < EPSILON);
    }

    Some(roots)
}

const BISECTION_MAX_ITER: usize = 100;

fn find_root_bisection_quintic(
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    mut start: f64,
    mut end: f64,
) -> f64 {
    let mut ya = eval_quintic(a, b, c, d, e, f, start);
    let yb = eval_quintic(a, b, c, d, e, f, end);

    if ya.abs() < EPSILON {
        return start;
    };
    if yb.abs() < EPSILON {
        return end;
    };

    // A root is only bracketed if the signs are different
    if ya * yb > 0. {
        return f64::NAN; // Not a number, indicates no root in this bracket
    }

    let mut mid = start;
    for _ in 0..BISECTION_MAX_ITER {
        mid = (start + end) / 2.0;
        let y_mid = eval_quintic(a, b, c, d, e, f, mid);

        if y_mid.abs() < EPSILON {
            return mid;
        }

        if ya * y_mid < 0. {
            start = mid;
        } else {
            end = mid;
            ya = y_mid;
        }
    }
    return mid;
}

fn solve_quartic(a: f64, b: f64, c: f64, d: f64, e: f64) -> Option<Vec<f64>> {
    if a.abs() < EPSILON {
        // Not a quartic
        return solve_cubic(b, c, d, e);
    }

    // Normalize to x^4 + ax^3 + bx^2 + cx + d = 0
    let A = b / a;
    let B = c / a;
    let C = d / a;
    let D = e / a;

    let p = B - 3.0 * A * A / 8.0;
    let q = C + A * A * A / 8.0 - A * B / 2.0;
    let r = D - 3.0 * A * A * A * A / 256.0 + A * A * B / 16.0 - A * C / 4.0;

    // Solve the resolvent cubic: z^3 - pz^2 - 4rz + (4pr - q^2) = 0
    let cubic_roots = match solve_cubic(1.0, -p, -4.0 * r, 4.0 * p * r - q * q) {
        Some(v) => v,
        None => return None,
    };

    // Find a suitable real root 'y' from the resolvent cubic
    let y = cubic_roots[0]; // Any real root will do
    //
    let mut R_sq = y - p;
    if R_sq < 0. {
        R_sq = 0.
    }; // Handle precision errors
    let R = R_sq.sqrt();

    let (mut S1, mut S2) = (0., 0.);
    if y * y - 4. * r >= 0. {
        S1 = (y * y - 4. * r).sqrt()
    };
    if y * y - 4. * r < 0. {
        S2 = (4. * r - y * y).sqrt()
    };

    let mut roots = Vec::with_capacity(4);

    for root in match solve_quadratic(1., R, (y + S1 + S2) / 2.0) {
        Some(v) => v,
        None => return None,
    }
    .into_iter()
    {
        roots.push(root - A / 4.);
    }
    for root in match solve_quadratic(1., -R, (y - S1 - S2) / 2.0) {
        Some(v) => v,
        None => return None,
    }
    .into_iter()
    {
        roots.push(root - A / 4.);
    }
    Some(roots)
}

#[derive(Debug, Clone)]
struct Contour<E> {
    edges: Vec<E>,
}

#[derive(Debug, Clone)]
struct Edge {
    segments: Vec<Segment>,
}

impl Edge {
    /// Checks if the segments are continously connected, order matters
    /// It is assumed that s0's end and s1's start are connected
    fn is_continous(s0: &Segment, s1: &Segment) -> bool {
        let s0_end_direction = s0.end_direction().norm();
        let s1_start_direction = s1.start_direction().norm();
        s0_end_direction.cross(s1_start_direction).abs() < EPSILON
            && (s0_end_direction.dot(s1_start_direction) - 1.).abs() < EPSILON
    }

    fn sdistance(&self, point: Vector) -> f64 {
        self.segments
            .iter()
            .map(|segment| segment.sdistance(point))
            .min_by(|x, y| x.abs().total_cmp(&y.abs()))
            .unwrap()
    }
}

impl From<Vec<Segment>> for Contour<Edge> {
    fn from(value: Vec<Segment>) -> Self {
        let mut edges: Vec<Vec<Segment>> = Vec::new();
        for segment in value.into_iter() {
            let last_edge = match edges.last_mut() {
                Some(e) => e,
                None => {
                    edges.push(vec![segment]);
                    continue;
                }
            };
            let last_segment = match last_edge.last() {
                Some(e) => e,
                None => {
                    last_edge.push(segment);
                    continue;
                }
            };
            if Edge::is_continous(last_segment, &segment) {
                last_edge.push(segment);
            } else {
                edges.push(vec![segment]);
            }
        }
        Self {
            edges: edges.into_iter().map(|s| Edge { segments: s }).collect(),
        }
    }
}

#[derive(Debug, Clone)]
struct Shape<E> {
    contours: Vec<Contour<E>>,
}

impl From<Vec<Segment>> for Shape<Edge> {
    /// Assumes that the Segments are in order
    fn from(value: Vec<Segment>) -> Self {
        let mut contours: Vec<Vec<Segment>> = Vec::new();
        for segment in value.into_iter() {
            let last_contour = match contours.last_mut() {
                Some(c) => c,
                None => {
                    contours.push(vec![segment]);
                    continue;
                }
            };

            let last_segment_end = match last_contour.last() {
                Some(s) => s.end(),
                None => {
                    // Should never happen, but handling just in case
                    last_contour.push(segment);
                    continue;
                }
            };
            if last_segment_end.sq_dist(segment.start()) < EPSILON {
                last_contour.push(segment)
            } else {
                contours.push(vec![segment]);
            }
        }
        Self {
            contours: contours.into_iter().map(|c| c.into()).collect(),
        }
    }
}

/// Construct a shape while scaling to fit the provided height
/// All vertices in the resulting shape are in ((0, outline.bounds.width() / (outline.bounds.height())), (0, 1))
/// Padding inclusive
struct ShapeBuilder {
    shape: Shape<Edge>,
    w: f64,
    outline: Outline,
}

impl ShapeBuilder {
    fn new(outline: Outline) -> Self {
        let scale = outline.bounds.height().abs() as f64;
        let segments: Vec<Segment> = outline
            .curves
            .clone()
            .into_iter()
            .map(|c| ((Segment::from(c.clone())) + (Vector::from(outline.bounds.min))) / scale)
            .collect();
        let shape: Shape<Edge> = segments.into();
        Self {
            shape: shape.into(),
            w: outline.bounds.width().abs() as f64 / scale,
            outline,
        }
    }

    fn width(&self, height: usize) -> usize {
        (self.w * height as f64).ceil() as usize
    }

    /// Returns the sdf, ready for rendering, and the width and height of the new
    /// shape.
    /// Padding is used to ensure that the distance field on the edges properly terminates
    /// The image will be rendered to a rectangle of (self.width * (height - 2 * padding), height - 2
    /// * padding)
    ///
    /// TODO: Do something clever to generate either msdf or generate a way to get sharp corners
    /// for text
    fn render<F: FnMut(usize, usize, u8)>(self, height: usize, padding: usize, mut write_pixel: F) {
        let width_f = self.w * height as f64;
        let height_f = height as f64;
        let inner_height = (height - 2 * padding) as f64;
        let inner_width = self.w * inner_height;
        let width = width_f.ceil() as usize;

        let max_distance = 2. * (width_f.powi(2) + height_f.powi(2)).sqrt();

        let horizontal_flipped = self.outline.bounds.width() < 0.;
        let vertical_flipped = self.outline.bounds.height() < 0.;

        for y in 0..height {
            for x in 0..width {
                let point = Vector {
                    x: ((x as isize) as f64 + 0.5) / inner_width,
                    y: ((y as isize + height as isize - 3 * padding as isize) as f64 + 0.5)
                        / inner_height,
                };
                let mut min_dist = max_distance;
                for contour in self.shape.contours.iter() {
                    for edge in contour.edges.iter() {
                        let d = edge.sdistance(point);
                        if d.abs() < min_dist.abs() {
                            min_dist = d;
                        }
                    }
                }
                write_pixel(
                    if horizontal_flipped { width - x - 1 } else { x },
                    if vertical_flipped { height - y - 1 } else { y },
                    (255. * ((min_dist) + 1.) / 2.) as u8,
                );
            }
        }
    }
}

pub fn generate_font_sdf(available_chars: &str) -> FontSDF {
    let font_ref = FontRef::try_from_slice(FONT_DATA).expect("The font to be a valid file");

    const PIX_HEIGHT: usize = 20;
    // TODO: Iterate and render and the characters instead of only one
    let (outlines, (width, height), _) = available_chars
        .chars()
        .map(|c| (c, font_ref.glyph_id(c)))
        .flat_map(|(c, id)| font_ref.outline(id).map(|outline| (c, outline)))
        .map(|(c, outline)| (c, ShapeBuilder::new(outline)))
        .map(|(c, shape_builder)| (c, shape_builder.width(PIX_HEIGHT), shape_builder))
        .fold(
            (HashMap::new(), (0, 0), (0, 0)),
            |(mut positions, (width, height), (x, y)), (c, w, shape_builder)| {
                let current_width = shape_builder.width(PIX_HEIGHT);
                positions.insert(c, (shape_builder, w, (x, y)));
                let x = x + current_width;
                let width = if x > width {
                    width + current_width
                } else {
                    width
                };
                (positions, (width, height), (x, height))
            },
        );

    let mut img = vec![0u8; width * (height + PIX_HEIGHT)];

    let mut locations = HashMap::new();

    for (c, (shape_builder, c_width, (bottom_right_x, bottom_right_y))) in outlines.into_iter() {
        shape_builder.render(PIX_HEIGHT, 2, |x, y, b| {
            img[width * (y + bottom_right_y) + (x + bottom_right_x)] = b;
        });
        locations.insert(
            c,
            (
                c_width as f64 / PIX_HEIGHT as f64,
                Rect {
                    min: Point {
                        x: (bottom_right_x) as f32 / width as f32,
                        y: (bottom_right_y) as f32 / (height + PIX_HEIGHT) as f32,
                    },
                    max: Point {
                        x: (bottom_right_x + c_width) as f32 / width as f32,
                        y: (bottom_right_y + PIX_HEIGHT) as f32 / (height + PIX_HEIGHT) as f32,
                    },
                },
            ),
        );
    }

    // The curves are, in fact, in order
    // From the code in ttf_glyph
    let mut temp_file = File::create("temp_img.data").unwrap();
    temp_file.write_all(bytemuck::cast_slice(&img)).unwrap();

    FontSDF {
        data: bytemuck::cast_slice(&img).to_vec(),
        width: width as u32,
        height: (height + PIX_HEIGHT) as u32,
        locations,
    }
}
