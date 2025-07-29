use std::{
    collections::HashMap,
    ops::{Add, Div, Mul, Sub},
};

use ab_glyph::{Font, FontRef, Outline, OutlineCurve, Point};

pub const FONT_DATA: &[u8] = include_bytes!("test_font.ttf");

#[derive(Debug)]
pub struct FontContainer {
    pub points_texture: CurvePointsTexture,
    pub curve_offsets: CurveOffsets,
    pub locations: HashMap<char, ShapeLocation>,
}

#[derive(Debug)]
pub struct ShapeLocation {
    pub offset: u32,
    pub len: u32,
    pub aspect_ratio: f32,
}

impl FontContainer {
    pub fn new(available_chars: &str) -> Self {
        let font_ref = FontRef::try_from_slice(FONT_DATA).expect("The font to be a valid file");
        let (points, curve_offsets, locations) = available_chars
            .chars()
            .map(|c| (c, font_ref.glyph_id(c)))
            .flat_map(|(c, id)| font_ref.outline(id).map(|outline| (c, outline)))
            .map(|(c, outline)| (c, Shape::from(outline)))
            .fold(
                (
                    Vec::new(),
                    CurveOffsets { data: Vec::new() },
                    HashMap::new(),
                ),
                |(mut points, mut offsets, mut locations), (c, shape)| {
                    let shape_offset = offsets.len();
                    for segment in shape.segments.into_iter() {
                        offsets.append(CurveOffset {
                            curve_type: segment.curve_type(),
                            offset: points.len() as u32,
                        });
                        points.extend_from_slice(segment.to_bytes().as_slice());
                    }
                    let shape_len = offsets.len() - shape_offset;
                    locations.insert(
                        c,
                        ShapeLocation {
                            offset: shape_offset,
                            len: shape_len,
                            aspect_ratio: shape.aspect_ratio,
                        },
                    );
                    (points, offsets, locations)
                },
            );
        Self {
            points_texture: CurvePointsTexture::from(points),
            curve_offsets,
            locations,
        }
    }
}

#[repr(u8)]
#[derive(Debug)]
pub enum CurveType {
    Line = 0u8,
    Bez2 = 1u8,
    Bez3 = 2u8,
}

#[derive(Debug)]
pub struct CurveOffsets {
    pub data: Vec<u8>,
}

impl CurveOffsets {
    fn append(&mut self, offset: CurveOffset) {
        self.data.push(offset.curve_type as u8);
        self.data
            .extend_from_slice(bytemuck::bytes_of(&offset.offset));
    }

    fn len(&self) -> u32 {
        self.data.len() as u32
    }
}

#[derive(Debug)]
pub struct CurveOffset {
    pub curve_type: CurveType,
    pub offset: u32,
}

#[derive(Debug)]
pub struct CurvePointsTexture {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl From<Vec<u8>> for CurvePointsTexture {
    /// The format is supposed to have 2 channels
    fn from(mut data: Vec<u8>) -> Self {
        let len = data.len() as u32 / 2;
        let width = len.isqrt();
        let mut height = width;
        // Dealing with the round down of isqrt
        while width * height < len {
            height += 1;
        }

        // Padding the data out so that GPU doesn't scream and die
        if (data.len() as u32) < width * height * 2 {
            data.extend((0..(width * height * 2 - data.len() as u32)).map(|_| 0u8));
        }

        CurvePointsTexture {
            data,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
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

impl Div for Vector {
    type Output = Vector;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

impl Add<f32> for Vector {
    type Output = Vector;

    fn add(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

const EPSILON: f32 = 3e-15;

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
        if mag.abs() < EPSILON {
            return Self { x: 0., y: 0. };
        }
        Self {
            x: self.x / mag,
            y: self.y / mag,
        }
    }

    fn cross(&self, s0: Vector) -> f32 {
        self.x * s0.y - self.y * s0.x
    }
}

// I think this means that there won't be a copy
impl From<Point> for Vector {
    fn from(value: Point) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Segment {
    LINE(Line),
    BEZ2(Bez2),
    BEZ3(Bez3),
}
impl Segment {
    fn curve_type(&self) -> CurveType {
        match self {
            Segment::LINE(_) => CurveType::Line,
            Segment::BEZ2(_) => CurveType::Bez2,
            Segment::BEZ3(_) => CurveType::Bez3,
        }
    }

    fn to_bytes(self) -> Vec<u8> {
        match self {
            Segment::LINE(line) => line.to_u8_bytes().to_vec(),
            Segment::BEZ2(bez2) => bez2.to_u8_bytes().to_vec(),
            Segment::BEZ3(bez3) => bez3.to_u8_bytes().to_vec(),
        }
    }
}

impl Div<f32> for Segment {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        match self {
            Segment::LINE(line) => Self::LINE(line / rhs),
            Segment::BEZ2(bez2) => Self::BEZ2(bez2 / rhs),
            Segment::BEZ3(bez3) => Self::BEZ3(bez3 / rhs),
        }
    }
}

impl Div<Vector> for Segment {
    type Output = Self;

    fn div(self, rhs: Vector) -> Self::Output {
        match self {
            Segment::LINE(line) => Self::LINE(line / rhs),
            Segment::BEZ2(bez2) => Self::BEZ2(bez2 / rhs),
            Segment::BEZ3(bez3) => Self::BEZ3(bez3 / rhs),
        }
    }
}

impl Add<f32> for Segment {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        match self {
            Segment::LINE(line) => Self::LINE(line + rhs),
            Segment::BEZ2(bez2) => Self::BEZ2(bez2 + rhs),
            Segment::BEZ3(bez3) => Self::BEZ3(bez3 + rhs),
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

impl Mul<f32> for Segment {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
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
pub struct Line(pub Vector, pub Vector);
impl Line {
    fn to_u8_bytes(self) -> [u8; 4] {
        [
            (self.0.x * 255.) as u8,
            (self.0.y * 255.) as u8,
            (self.1.x * 255.) as u8,
            (self.1.y * 255.) as u8,
        ]
    }
}

impl Div<f32> for Line {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs)
    }
}

impl Div<Vector> for Line {
    type Output = Self;

    fn div(self, rhs: Vector) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs)
    }
}

impl Add<f32> for Line {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs)
    }
}

#[derive(Clone, Copy, Debug)]
struct Bez2(Vector, Vector, Vector);
impl Bez2 {
    fn to_u8_bytes(&self) -> [u8; 6] {
        [
            (self.0.x * 255.) as u8,
            (self.0.y * 255.) as u8,
            (self.1.x * 255.) as u8,
            (self.1.y * 255.) as u8,
            (self.2.x * 255.) as u8,
            (self.2.y * 255.) as u8,
        ]
    }
}

impl Div<f32> for Bez2 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs, self.2 / rhs)
    }
}

impl Div<Vector> for Bez2 {
    type Output = Self;

    fn div(self, rhs: Vector) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs, self.2 / rhs)
    }
}

impl Add<f32> for Bez2 {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs, self.2 + rhs)
    }
}

#[derive(Clone, Copy, Debug)]
struct Bez3(Vector, Vector, Vector, Vector);
impl Bez3 {
    fn to_u8_bytes(&self) -> [u8; 8] {
        [
            (self.0.x * 255.) as u8,
            (self.0.y * 255.) as u8,
            (self.1.x * 255.) as u8,
            (self.1.y * 255.) as u8,
            (self.2.x * 255.) as u8,
            (self.2.y * 255.) as u8,
            (self.3.x * 255.) as u8,
            (self.3.y * 255.) as u8,
        ]
    }
}

impl Div<f32> for Bez3 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs, self.2 / rhs, self.3 / rhs)
    }
}

impl Div<Vector> for Bez3 {
    type Output = Self;

    fn div(self, rhs: Vector) -> Self::Output {
        Self(self.0 / rhs, self.1 / rhs, self.2 / rhs, self.3 / rhs)
    }
}

impl Add<f32> for Bez3 {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        Self(self.0 + rhs, self.1 + rhs, self.2 + rhs, self.3 + rhs)
    }
}

#[derive(Debug, Clone)]
pub struct Shape {
    segments: Vec<Segment>,
    aspect_ratio: f32,
}

impl From<Outline> for Shape {
    /// Assumes that the Segments are in order
    fn from(value: Outline) -> Self {
        let scaling_vector = Vector {
            x: value.bounds.width(),
            y: value.bounds.height(),
        };
        let offset_vector = Vector {
            x: -value.bounds.min.x.min(value.bounds.max.x),
            y: -value.bounds.min.y.min(value.bounds.max.y),
        };
        Self {
            aspect_ratio: value.bounds.width() / value.bounds.height(),
            segments: value
                .curves
                .into_iter()
                .map(|outline_curve| Segment::from(outline_curve))
                .map(|segment| (segment + offset_vector) / scaling_vector)
                .collect(),
        }
    }
}
