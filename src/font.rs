use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    ops::{Add, Div, Mul, Sub},
};

use ab_glyph::{Font, FontRef, OutlineCurve, Point, Rect};

pub const FONT_DATA: &[u8] = include_bytes!("test_font.ttf");

#[derive(Debug)]
pub struct FontSDF {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub locations: HashMap<char, Rect>,
}

#[derive(Clone, Copy, Debug)]
struct Vector {
    x: f32,
    y: f32,
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

impl<F: Into<f32>> Mul<F> for Vector {
    type Output = Self;

    fn mul(self, rhs: F) -> Self::Output {
        let v = rhs.into();
        Self {
            x: self.x * v,
            y: self.y * v,
        }
    }
}

impl<F: Into<f32>> Div<F> for Vector {
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
    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    fn sq_dist(self, rhs: Self) -> f32 {
        (rhs.x - self.x).powi(2) + (rhs.y - self.y).powi(2)
    }

    fn mag(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn norm(self) -> Self {
        let mag = self.mag();
        Self {
            x: self.x / mag,
            y: self.y / mag,
        }
    }

    fn cross(&self, s0: Vector) -> f32 {
        self.x * s0.y - self.y * s0.x
    }
}

impl From<Point> for Vector {
    fn from(value: Point) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Segment {
    LINE(Line),
    BEZ2(Bez2),
    BEZ3(Bez3),
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

#[derive(Clone, Copy, Debug)]
struct Bez2(Vector, Vector, Vector);

#[derive(Clone, Copy, Debug)]
struct Bez3(Vector, Vector, Vector, Vector);

#[derive(Debug, Clone)]
struct Contour {
    edges: Vec<Edge>,
}

impl Contour {
    fn end(&self) -> Option<Segment> {
        self.edges.last().map(|e| e.end()).flatten()
    }
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
        s0_end_direction.cross(s1_start_direction).abs() < 0.001
            && s0_end_direction.dot(s1_start_direction).abs() - 1. < 0.001
    }

    fn end(&self) -> Option<Segment> {
        self.segments.last().copied()
    }
}

impl From<Vec<Segment>> for Contour {
    fn from(mut value: Vec<Segment>) -> Self {
        let mut edges: Vec<Vec<Segment>> = Vec::new();
        for segment in value.into_iter() {
            let last_segment = match edges.last().map(|c| c.last()).flatten() {
                Some(s) => s,
                None => {
                    edges.push(vec![segment]);
                    continue;
                }
            };
            if Edge::is_continous(last_segment, &segment) {
                match edges.last_mut() {
                    Some(s) => {
                        s.push(segment);
                    }
                    None => {
                        edges.push(vec![segment]);
                        continue;
                    }
                };
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
struct Shape {
    contours: Vec<Contour>,
}

impl From<Vec<Segment>> for Shape {
    /// Assumes that the Segments are in order
    fn from(value: Vec<Segment>) -> Self {
        let mut contours: Vec<Vec<Segment>> = Vec::new();
        for segment in value.into_iter() {
            let last_segment_end = match contours.last().map(|c| c.last()).flatten() {
                Some(s) => s.end(),
                None => {
                    contours.push(vec![segment]);
                    continue;
                }
            };
            if last_segment_end.sq_dist(segment.start()) < 0.001 {
                match contours.last_mut() {
                    Some(s) => {
                        s.push(segment);
                    }
                    None => {
                        contours.push(vec![segment]);
                        continue;
                    }
                };
            } else {
                contours.push(vec![segment]);
            }
        }
        Self {
            contours: contours.into_iter().map(|c| c.into()).collect(),
        }
    }
}

pub fn generate_font_sdf(available_chars: &str) -> FontSDF {
    let font_ref = FontRef::try_from_slice(FONT_DATA).expect("The font to be a valid file");

    // TODO: Iterate and render and the characters instead of only one
    let c = available_chars.chars().next().unwrap();

    let c_id = font_ref.glyph_id(c);
    // TODO: Handle the no outline case as well
    let c_outline = font_ref.outline(c_id).unwrap();

    let c_bounds = c_outline.bounds;
    let width_f = c_bounds.width().abs() + 1.;
    let height_f = c_bounds.height().abs() + 1.;
    let width = width_f as usize;
    let height = height_f as usize;
    let mut img = vec![core::u8::MAX; width * height];
    let mut locations = HashMap::new();
    // The curves are, in fact, in order
    // From the code in ttf_glyph
    let segments: Vec<Segment> = c_outline.curves.iter().map(|c| c.clone().into()).collect();
    let shape: Shape = segments.into();
    locations.insert(
        c,
        Rect {
            min: Point { x: 0., y: 0. },
            max: Point { x: 0., y: 0. },
        },
    );

    let mut f = File::create("font_sdf.data").unwrap();
    f.write_all(&img).unwrap();

    FontSDF {
        data: img,
        width: width as u32,
        height: height as u32,
        locations,
    }
}
