use std::{
    collections::HashMap,
    ops::{Add, Div, Mul, Sub},
};

use ab_glyph::{Font, FontArc, FontRef, GlyphId, OutlineCurve, Point};

pub const FONT_DATA: &[u8] = include_bytes!("test_font.ttf");

#[derive(Debug, Clone)]
pub struct FontContainer {
    /// This texture holds the points for lines
    pub linear_points_buffer: Vec<f32>,
    /// This texture holds the points for quadratic bezier curves
    pub quadratic_points_buffer: Vec<f32>,
    /// This texture holds the points for cubic bezier curves
    pub cubic_points_buffer: Vec<f32>,

    /// Offsets for the curve points in the texture defined above
    /// For the units of offset, refer to ShapeLocation::offset
    // TODO: refactor so that the texture and offests are in a single struct
    pub line_curve_offsets: Vec<u32>,
    pub quadratic_curve_offsets: Vec<u32>,
    pub cubic_curve_offsets: Vec<u32>,

    /// Locations of characters in the curve_offsets, defined in curve_offsets
    pub locations: HashMap<char, GlyphInfo>,

    /// The original font parsed into a struct
    pub font_arc: FontArc,

    pub char_map: HashMap<GlyphId, char>,
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
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    pub line_off: GlyphOffLen,
    pub bez2_off: GlyphOffLen,
    pub bez3_off: GlyphOffLen,

    /// Normalized dimensions in 0..1 range
    pub dimensions: Vec2,

    /// Normalized offset in 0..1 range
    pub offset: Vec2,

    /// GlyphId corresponding to the font
    pub glyph_id: GlyphId,

    pub advance: f32,
}

impl FontContainer {
    pub fn new(available_chars: &str) -> Self {
        let font_arc = FontArc::try_from_slice(FONT_DATA).expect("The font to be a valid file");
        let units_per_em = font_arc.units_per_em().unwrap_or(16384.0);
        let char_map = HashMap::from_iter(font_arc.codepoint_ids());
        let (
            (line_points, quadratic_points, cubic_points),
            (line_curve_offsets, quadratic_curve_offsets, cubic_curve_offsets),
            locations,
        ) = available_chars
            .chars()
            .map(|c| (c, font_arc.glyph_id(c)))
            .flat_map(|(c, id)| Shape::from_glyph(font_arc.clone(), id).map(|shape| (c, shape, id)))
            .fold(
                (
                    (Vec::<Line>::new(), Vec::<Bez2>::new(), Vec::<Bez3>::new()),
                    (Vec::<u32>::new(), Vec::<u32>::new(), Vec::<u32>::new()),
                    HashMap::new(),
                ),
                |(mut segments, mut offsets, mut locations), (c, shape, glyph_id)| {
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
                            glyph_id,
                            advance: font_arc.h_advance_unscaled(glyph_id) / units_per_em,
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
                            dimensions: shape.dimensions,
                            offset: shape.offset,
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
            char_map,
            linear_points_buffer: line_points
                .clone()
                .into_iter()
                .flat_map(|v| v.to_f32_arr())
                .collect(),
            quadratic_points_buffer: quadratic_points
                .clone()
                .into_iter()
                .flat_map(|v| v.to_f32_arr())
                .collect(),
            cubic_points_buffer: cubic_points
                .clone()
                .into_iter()
                .flat_map(|v| v.to_f32_arr())
                .collect(),
            line_curve_offsets,
            quadratic_curve_offsets,
            cubic_curve_offsets,
            locations,
            font_arc: font_arc.into(),
        }
    }

    pub fn load_char_with_id(&mut self, id: GlyphId) -> Option<GlyphInfo> {
            match self.char_map.get(&id) {
                Some(x) => return self.load_char(*x),
                None => None,
            }
    }

    pub fn load_char(&mut self, c: char) -> Option<GlyphInfo> {
        let units_per_em = self.font_arc.units_per_em().unwrap_or(16384.0);
        if let Some(x) = self.locations.get(&c) {
            return Some(*x);
        }
        let glyph_id = self.font_arc.glyph_id(c);
        let shape = match Shape::from_glyph(self.font_arc.clone(), glyph_id) {
            Some(x) => x,
            None => return None,
        };

        let (lines_offset, bez2_offset, bez3_offset) = (
            self.linear_points_buffer.len() as u32 / 4,
            self.quadratic_points_buffer.len() as u32 / 6,
            self.cubic_points_buffer.len() as u32 / 8,
        );

        for segment in shape.segments.into_iter() {
            match segment {
                Segment::LINE(line) => {
                    self.line_curve_offsets
                        .push(self.linear_points_buffer.len() as u32 / 4);
                    self.linear_points_buffer.extend(line.to_f32_arr());
                }
                Segment::BEZ2(bez2) => {
                    self.quadratic_curve_offsets
                        .push(self.quadratic_points_buffer.len() as u32 / 6);
                    self.quadratic_points_buffer.extend(bez2.to_f32_arr());
                }
                Segment::BEZ3(bez3) => {
                    self.cubic_curve_offsets
                        .push(self.cubic_points_buffer.len() as u32 / 8);
                    self.cubic_points_buffer.extend(bez3.to_f32_arr());
                }
            }
        }
        let glyph_info = GlyphInfo {
            glyph_id,
            advance: self.font_arc.h_advance_unscaled(glyph_id) / units_per_em,
            line_off: GlyphOffLen {
                position: lines_offset,
                len: self.linear_points_buffer.len() as u32 / 4 - lines_offset,
            },
            bez2_off: GlyphOffLen {
                position: bez2_offset,
                len: self.quadratic_points_buffer.len() as u32 / 6 - bez2_offset,
            },
            bez3_off: GlyphOffLen {
                position: bez3_offset,
                len: self.cubic_points_buffer.len() as u32 / 8 - bez3_offset,
            },
            offset: shape.offset,
            dimensions: shape.dimensions,
        };
        self.locations.insert(c, glyph_info);

        Some(glyph_info)
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
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}
impl Vec2 {
    fn mag(&self) -> f32 {
        (self.y * self.y + self.x * self.x).sqrt()
    }
}

impl Add for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: rhs.x + self.x,
            y: rhs.y + self.y,
        }
    }
}

impl Div for Vec2 {
    type Output = Vec2;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

impl Add<f32> for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

impl<F: Into<f32>> Mul<F> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: F) -> Self::Output {
        let v = rhs.into();
        Self {
            x: self.x * v,
            y: self.y * v,
        }
    }
}

impl<F: Into<f32>> Div<F> for Vec2 {
    type Output = Self;

    fn div(self, rhs: F) -> Self::Output {
        let rhs = rhs.into();
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl Sub for Vec2 {
    type Output = Vec2;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

// I think this means that there won't be a copy
impl From<Point> for Vec2 {
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

impl Div<Vec2> for Segment {
    type Output = Self;

    fn div(self, rhs: Vec2) -> Self::Output {
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

impl Add<Vec2> for Segment {
    type Output = Self;

    fn add(self, rhs: Vec2) -> Self::Output {
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
pub struct Line(pub Vec2, pub Vec2);

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

impl Div<Vec2> for Line {
    type Output = Self;

    fn div(self, rhs: Vec2) -> Self::Output {
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
pub struct Bez2(Vec2, Vec2, Vec2);
impl Bez2 {
    fn to_f32_arr(&self) -> [f32; 6] {
        [self.0.x, self.0.y, self.1.x, self.1.y, self.2.x, self.2.y]
    }

    fn eval(&self, t: f32) -> Vec2 {
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

impl Div<Vec2> for Bez2 {
    type Output = Self;

    fn div(self, rhs: Vec2) -> Self::Output {
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
pub struct Bez3(Vec2, Vec2, Vec2, Vec2);
impl Bez3 {
    fn to_f32_arr(&self) -> [f32; 8] {
        [
            self.0.x, self.0.y, self.1.x, self.1.y, self.2.x, self.2.y, self.3.x, self.3.y,
        ]
    }

    fn eval(&self, t: f32) -> Vec2 {
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

impl Div<Vec2> for Bez3 {
    type Output = Self;

    fn div(self, rhs: Vec2) -> Self::Output {
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
    dimensions: Vec2,
    offset: Vec2,
}

impl Shape {
    fn from_glyph(font_arc: FontArc, glyph_id: GlyphId) -> Option<Self> {
        let units_per_em = font_arc.units_per_em().unwrap_or(16384.0);

        let outline = match font_arc.outline(glyph_id) {
            Some(x) => x,
            None => return None,
        };

        let bounds = outline.bounds;

        let scaling_vector = Vec2 {
            x: bounds.width(),
            y: bounds.height(),
        };

        let offset_vector = Vec2::from(outline.bounds.min) * -1.;

        let padding_scale = Vec2 {
            x: 1. / 0.8,
            y: 1. / 0.8,
        };
        let padding_offset = Vec2 { x: 0.1, y: 0.1 };

        Some(Self {
            dimensions: Vec2 {
                x: scaling_vector.x / units_per_em,
                y: scaling_vector.y / units_per_em,
            },
            offset: Vec2 {
                x: bounds.min.x / units_per_em,
                y: bounds.min.y / units_per_em,
            },
            segments: outline
                .curves
                .into_iter()
                .map(|outline_curve| Segment::from(outline_curve))
                .filter(|segment| segment.length_gte(1.))
                .map(|segment| (segment + offset_vector) / scaling_vector)
                .map(|segment| (segment / padding_scale) + padding_offset)
                .collect(),
        })
    }
}
