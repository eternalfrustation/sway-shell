use std::{
    collections::HashMap,
    ops::{Add, Div, Mul, Sub},
};

use ab_glyph::{Font, FontRef, Outline, OutlineCurve, Point};

pub const FONT_DATA: &[u8] = include_bytes!("test_font.ttf");

#[derive(Debug)]
pub struct FontContainer {
    /// This texture holds the points for lines
    pub linear_points_texture: Vec<f32>,
    /// This texture holds the points for quadratic bezier curves
    pub quadratic_points_texture: Vec<f32>,
    /// This texture holds the points for cubic bezier curves
    pub cubic_points_texture: Vec<f32>,

    /// Offsets for the curve points in the texture defined above
    /// For the units of offset, refer to ShapeLocation::offset
    // TODO: refactor so that the texture and offests are in a single struct
    pub line_curve_offsets: Vec<u32>,
    pub quadratic_curve_offsets: Vec<u32>,
    pub cubic_curve_offsets: Vec<u32>,

    /// Locations of characters in the curve_offsets, defined in curve_offsets
    pub locations: HashMap<char, GlyphInfo>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphOffLen {
    /// offset is in terms of primitives, NOT in terms of bytes
    /// Primitives are: Line, Bez2, Bez3, so a offset of 3 would mean skipping
    /// 3 * 4 = 12 bytes for lines
    /// 3 * 6 = 18 bytes for bez2
    /// 3 * 8 = 24 bytes for bez3
    pub position: u32,
    /// len is in terms of primitives, NOT in terms of bytes, refer to offset for an example
    pub len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlyphInfo {
    pub line_off: GlyphOffLen,
    pub bez2_off: GlyphOffLen,
    pub bez3_off: GlyphOffLen,
    /// aspect ratio for this shape
    /// Since all shapes are normalized, you need this to get the proper width
    pub aspect_ratio: f32,
}

impl FontContainer {
    pub fn new(available_chars: &str) -> Self {
        let font_ref = FontRef::try_from_slice(FONT_DATA).expect("The font to be a valid file");
        let (
            (line_points, quadratic_points, cubic_points),
            (line_curve_offsets, quadratic_curve_offsets, cubic_curve_offsets),
            locations,
        ) = available_chars
            .chars()
            .map(|c| (c, font_ref.glyph_id(c)))
            .flat_map(|(c, id)| font_ref.outline(id).map(|outline| (c, outline)))
            .map(|(c, outline)| (c, Shape::from(outline)))
            .fold(
                (
                    (Vec::<Line>::new(), Vec::<Bez2>::new(), Vec::<Bez3>::new()),
                    (Vec::<u32>::new(), Vec::<u32>::new(), Vec::<u32>::new()),
                    HashMap::new(),
                ),
                |(mut segments, mut offsets, mut locations), (c, shape)| {
                    let (lines_offset, bez2_offset, bez3_offset) = (
                        offsets.0.len() as u32,
                        offsets.1.len() as u32,
                        offsets.2.len() as u32,
                    );
                    for segment in shape.segments.into_iter() {
                        match segment {
                            Segment::LINE(line) => {
                                offsets.0.push(segments.0.len() as u32);
                                segments.0.push(line)
                            }
                            Segment::BEZ2(bez2) => {
                                offsets.1.push(segments.1.len() as u32);
                                segments.1.push(bez2)
                            }
                            Segment::BEZ3(bez3) => {
                                offsets.2.push(segments.2.len() as u32);
                                segments.2.push(bez3)
                            }
                        }
                    }
                    locations.insert(
                        c,
                        GlyphInfo {
                            line_off: GlyphOffLen {
                                position: lines_offset,
                                len: segments.0.len() as u32 - lines_offset,
                            },
                            bez2_off: GlyphOffLen {
                                position: bez2_offset,
                                len: segments.1.len() as u32 - bez2_offset,
                            },
                            bez3_off: GlyphOffLen {
                                position: bez3_offset,
                                len: segments.2.len() as u32 - bez3_offset,
                            },
                            aspect_ratio: shape.aspect_ratio,
                        },
                    );
                    (segments, offsets, locations)
                },
            );
        /*test_svg_from_locations(
            &locations,
            line_points
                .clone()
                .into_iter()
                .flat_map(|v| v.to_f32_arr())
                .collect(),
            quadratic_points
                .clone()
                .into_iter()
                .flat_map(|v| v.to_f32_arr())
                .collect(),
            '1',
        );
        dbg!(locations[&'1']);
*/
        Self {
            linear_points_texture: if line_points.len() == 0 {
                vec![0., 0., 0., 0.]
            } else {
                line_points
                    .clone()
                    .into_iter()
                    .flat_map(|v| v.to_f32_arr())
                    .collect()
            },
            quadratic_points_texture: if quadratic_points.len() == 0 {
                vec![0., 0., 0., 0., 0., 0.]
            } else {
                quadratic_points
                    .clone()
                    .into_iter()
                    .flat_map(|v| v.to_f32_arr())
                    .collect()
            },
            cubic_points_texture: if cubic_points.len() == 0 {
                vec![0., 0., 0., 0., 0., 0., 0., 0.]
            } else {
                cubic_points
                    .clone()
                    .into_iter()
                    .flat_map(|v| v.to_f32_arr())
                    .collect()
            },
            line_curve_offsets: if line_curve_offsets.len() == 0 {
                vec![0]
            } else {
                line_curve_offsets
            },
            quadratic_curve_offsets: if quadratic_curve_offsets.len() == 0 {
                vec![0]
            } else {
                quadratic_curve_offsets
            },
            cubic_curve_offsets: if cubic_curve_offsets.len() == 0 {
                vec![0]
            } else {
                cubic_curve_offsets
            },
            locations,
        }
    }
}

/*
fn test_svg_from_locations(
    locations: &HashMap<char, GlyphInfo>,
    line_buf: Vec<f32>,
    quad_buf: Vec<f32>,
    c: char,
) {
    let position = locations[&c];
    let mut document = svg::Document::new().set("viewbox", (0., 0., 1000., 1000.)).add(
        svg::node::element::Definitions::new().add(
            svg::node::element::Marker::new()
                .set("id", "arrow_tip")
                .set("viewbox", (0, 0, 10, 10))
                .set("refX", 5)
                .set("refY", 5)
                .set("markerWidth", 1)
                .set("markerHeight", 1)
                .set("orient", "auto-start-reverse")
                .add(
                    svg::node::element::Path::new().set(
                        "d",
                        svg::node::element::path::Data::new()
                            .move_to((0, 0))
                            .line_to((10, 5))
                            .line_to((0, 10)),
                    ),
                ),
        ),
    );
    let lines_offset = position.line_off;
    let quad_offset = position.bez2_off;
    for i in lines_offset.position..(lines_offset.position + lines_offset.len) {
        let idx = i * 4;
        let x0 = line_buf[idx as usize];
        let y0 = line_buf[idx as usize + 1];
        let p0 = Vector { x: x0, y: y0 } * 1000.;
        let x1 = line_buf[idx as usize + 2];
        let y1 = line_buf[idx as usize + 3];
        let p1 = Vector { x: x1, y: y1 } * 1000.;
        document = document.add(
            svg::node::element::Path::new()
                .set("stroke", "black")
                .set("stroke-width", 4)
                .set("marker-end", "url(#arrow_tip)")
                .set(
                    "d",
                    svg::node::element::path::Data::new()
                        .move_to((p0.x, p0.y))
                        .line_to((p1.x, p1.y)),
                ),
        )
    }
    for i in quad_offset.position..(quad_offset.position + quad_offset.len) {
        let idx = i * 6;
        let x0 = quad_buf[idx as usize];
        let y0 = quad_buf[idx as usize + 1];
        let p0 = Vector { x: x0, y: y0 } * 1000.;
        let x1 = quad_buf[idx as usize + 2];
        let y1 = quad_buf[idx as usize + 3];
        let p1 = Vector { x: x1, y: y1 } * 1000.;
        let x2 = quad_buf[idx as usize + 4];
        let y2 = quad_buf[idx as usize + 5];
        let p2 = Vector { x: x2, y: y2 } * 1000.;
        dbg!(p0, p1);
        document = document.add(
            svg::node::element::Path::new()
                .set("fill", "none")
                .set("stroke", "black")
                .set("stroke-width", 0.001)
                .set("marker-end", "url(#arrow_tip)")
                .set(
                    "d",
                    svg::node::element::path::Data::new()
                        .move_to((p0.x, p0.y))
                        .smooth_quadratic_curve_to((p1.x, p1.y, p2.x, p2.y)),
                ),
        )
    }
    svg::save(format!("{c}.svg"), &document).unwrap()
}
*/

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}
impl Vector {
    fn mag(&self) -> f32 {
        (self.y * self.y + self.x * self.x).sqrt()
    }
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
    fn length_gte(&self, arg: f32) -> bool {
        match self {
            Segment::LINE(line) => line.length_gte(arg),
            Segment::BEZ2(bez2) => bez2.length_gte(arg),
            Segment::BEZ3(bez3) => bez3.length_gte(arg),
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

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Line(pub Vector, pub Vector);

impl Line {
    fn to_f32_arr(self) -> [f32; 4] {
        [self.0.x, self.0.y, self.1.x, self.1.y]
    }

    fn length_gte(&self, arg: f32) -> bool {
        (self.1 - self.0).mag() > arg
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

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Bez2(Vector, Vector, Vector);
impl Bez2 {
    fn to_f32_arr(&self) -> [f32; 6] {
        [self.0.x, self.0.y, self.1.x, self.1.y, self.2.x, self.2.y]
    }

    fn eval(&self, t: f32) -> Vector {
        self.0 * (1. - t) * (1. - t) + self.1 * t * (1. - t) + self.2 * t * t
    }

    fn length_gte(&self, arg: f32) -> bool {
        let middleish = self.eval(0.5);
        ((self.0 - middleish).mag().abs() + (self.1 - middleish).mag().abs()) > arg
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

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Bez3(Vector, Vector, Vector, Vector);
impl Bez3 {
    fn to_f32_arr(&self) -> [f32; 8] {
        [
            self.0.x, self.0.y, self.1.x, self.1.y, self.2.x, self.2.y, self.3.x, self.3.y,
        ]
    }

    fn eval(&self, t: f32) -> Vector {
        self.0 * (1. - t) * (1. - t) * (1. - t)
            + self.1 * t * (1. - t) * (1. - t)
            + self.2 * t * t * (1. - t)
            + self.3 * t * t * t
    }

    fn length_gte(&self, arg: f32) -> bool {
        let middleish = self.eval(0.5);
        ((self.0 - middleish).mag().abs() + (self.1 - middleish).mag().abs()) > arg
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
        let offset_vector = Vector::from(value.bounds.min) * -1.;
        let padding_scale = Vector {
            x: 1. / 0.8,
            y: 1. / 0.8,
        };
        let padding_offset = Vector { x: 0.1, y: 0.1 };
        Self {
            aspect_ratio: value.bounds.width() / value.bounds.height(),
            segments: value
                .curves
                .into_iter()
                .map(|outline_curve| Segment::from(outline_curve))
                .filter(|segment| segment.length_gte(1.))
                .map(|segment| (segment + offset_vector) / scaling_vector)
                .map(|segment| (segment / padding_scale) + padding_offset)
                .collect(),
        }
    }
}
