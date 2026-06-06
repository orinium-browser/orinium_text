/// RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

/// Font style variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Font weight (100–900).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(pub u16);

/// Visual style for a run of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: Color,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            color: Color(0, 0, 0, 255),
            font_weight: FontWeight(400),
            font_style: FontStyle::Normal,
            line_height: 1.2,
        }
    }
}

/// Unique identifier for a loaded font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontKey(pub fontdb::ID);

/// A single positioned glyph in a laid-out line.
#[derive(Debug, Clone)]
pub struct LayoutGlyph {
    pub glyph_id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

/// A single line of laid-out text.
#[derive(Debug, Clone)]
pub struct LayoutLine {
    pub glyphs: Vec<LayoutGlyph>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
}

/// Complete result of laying out a text string.
#[derive(Debug, Clone)]
pub struct TextLayout {
    pub lines: Vec<LayoutLine>,
    pub width: f32,
    pub height: f32,
}
