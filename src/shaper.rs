use rustybuzz::{Face, GlyphBuffer, UnicodeBuffer};

use crate::font::FONT_DATA;

/// A shaped glyph with its positioning information.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// The glyph ID from the font
    pub glyph_id: u32,
    /// The cluster index (which character in the original text this glyph corresponds to)
    pub cluster: u32,
    /// How much to advance horizontally after drawing this glyph (in font units)
    pub x_advance: i32,
    /// How much to advance vertically after drawing this glyph (in font units)
    pub y_advance: i32,
    /// How much to offset the glyph horizontally before drawing (in font units)
    pub x_offset: i32,
    /// How much to offset the glyph vertically before drawing (in font units)
    pub y_offset: i32,
}

/// Text shaper using HarfBuzz (via rustybuzz) for proper text layout.
///
/// This replaces the manual heuristic-based layout with proper OpenType shaping
/// that handles kerning, ligatures, and other font features correctly.
pub struct TextShaper<'a> {
    face: Face<'a>,
    /// Font units per em, used for normalizing positions
    units_per_em: u16,
}

impl<'a> TextShaper<'a> {
    /// Create a new TextShaper from the embedded font data.
    pub fn new() -> Option<Self> {
        let face = Face::from_slice(FONT_DATA, 0)?;
        let units_per_em = face.units_per_em() as u16;
        Some(Self { face, units_per_em })
    }

    /// Create a new TextShaper from custom font data.
    pub fn from_font_data(font_data: &'a [u8], face_index: u32) -> Option<Self> {
        let face = Face::from_slice(font_data, face_index)?;
        let units_per_em = face.units_per_em() as u16;
        Some(Self { face, units_per_em })
    }

    /// Get the font's units per em value.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Shape a text string and return the shaped glyphs with proper positioning.
    ///
    /// Uses HarfBuzz's shaping algorithm to handle:
    /// - Kerning (spacing adjustments between specific glyph pairs)
    /// - Ligatures (combining characters like "fi" into a single glyph)
    /// - Complex script shaping (for scripts like Arabic, Devanagari, etc.)
    /// - OpenType features
    pub fn shape(&self, text: &str) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);

        // Shape the text using HarfBuzz
        let glyph_buffer: GlyphBuffer = rustybuzz::shape(&self.face, &[], buffer);

        // Extract the shaped glyph information
        let glyph_infos = glyph_buffer.glyph_infos();
        let glyph_positions = glyph_buffer.glyph_positions();

        glyph_infos
            .iter()
            .zip(glyph_positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                glyph_id: info.glyph_id,
                cluster: info.cluster,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
            })
            .collect()
    }

    /// Shape text and return normalized positions (in 0..1 range relative to em).
    ///
    /// This is useful when you need positions that are independent of font size.
    pub fn shape_normalized(&self, text: &str) -> Vec<(ShapedGlyph, f32, f32)> {
        let shaped = self.shape(text);
        let upem = self.units_per_em as f32;
        let mut x_cursor = 0.0f32;
        let mut y_cursor = 0.0f32;

        shaped
            .into_iter()
            .map(|glyph| {
                // Calculate the position for this glyph
                let glyph_x = x_cursor + (glyph.x_offset as f32 / upem);
                let glyph_y = y_cursor + (glyph.y_offset as f32 / upem);

                // Advance the cursor
                x_cursor += glyph.x_advance as f32 / upem;
                y_cursor += glyph.y_advance as f32 / upem;

                (glyph, glyph_x, glyph_y)
            })
            .collect()
    }

    /// Get the glyph ID for a character (if it exists in the font).
    pub fn glyph_index(&self, c: char) -> Option<u16> {
        self.face.glyph_index(c).map(|gid| gid.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shaper_creation() {
        let shaper = TextShaper::new();
        assert!(shaper.is_some(), "Should be able to create shaper from embedded font");
    }

    #[test]
    fn test_shape_empty_string() {
        let shaper = TextShaper::new().unwrap();
        let shaped = shaper.shape("");
        assert!(shaped.is_empty(), "Empty string should produce no glyphs");
    }

    #[test]
    fn test_shape_simple_text() {
        let shaper = TextShaper::new().unwrap();
        let shaped = shaper.shape("Hello");
        assert_eq!(shaped.len(), 5, "Hello should produce 5 glyphs");

        // All glyphs should have positive x_advance (for left-to-right text)
        for glyph in &shaped {
            assert!(glyph.x_advance > 0, "Each glyph should have positive x_advance");
        }
    }

    #[test]
    fn test_shape_normalized() {
        let shaper = TextShaper::new().unwrap();
        let shaped = shaper.shape_normalized("AB");
        assert_eq!(shaped.len(), 2, "AB should produce 2 glyphs");

        // First glyph should be at x=0
        assert_eq!(shaped[0].1, 0.0, "First glyph should be at x=0");

        // Second glyph should be after the first one's advance
        assert!(shaped[1].1 > 0.0, "Second glyph should be positioned after first");
    }
}
