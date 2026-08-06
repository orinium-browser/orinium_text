/// RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

/// Font style variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Typographic variant (subscript / superscript).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontVariant {
    Normal,
    Subscript,
    Superscript,
}

impl FontVariant {
    /// Scale factor relative to the declared `font_size`.
    pub fn scale(&self) -> f32 {
        match self {
            FontVariant::Normal => 1.0,
            FontVariant::Subscript | FontVariant::Superscript => 0.65,
        }
    }

    /// Baseline shift in font-size-relative units (positive = down).
    pub fn baseline_shift(&self) -> f32 {
        match self {
            FontVariant::Normal => 0.0,
            FontVariant::Subscript => 0.15,
            FontVariant::Superscript => -0.35,
        }
    }
}

/// Font weight (100–900).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(pub u16);

/// Controls how the bidirectional text algorithm determines the base paragraph direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BidiMode {
    Auto,
    Ltr,
    Rtl,
}

/// Visual style for a run of text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle<'a> {
    pub font_size: f32,
    pub color: Color,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub line_height: f32,
    pub bidi_mode: BidiMode,
    /// Font families to try in order. The first family that provides a matching
    /// font weight/style is used as the primary; characters missing from it
    /// fall back to subsequent families.
    pub font_families: Vec<fontdb::Family<'a>>,
    /// Exact font keys to try before `font_families`. Each font is used directly
    /// (no family-name query) and acts as a higher-priority entry in the
    /// fallback chain. Useful for web fonts loaded via `FontSystem::load_font_data`.
    pub exact_fonts: Vec<FontKey>,
    /// Typographic variant (normal / subscript / superscript).
    pub variant: FontVariant,
}

impl Default for TextStyle<'_> {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            color: Color(0, 0, 0, 255),
            font_weight: FontWeight(400),
            font_style: FontStyle::Normal,
            line_height: 1.2,
            bidi_mode: BidiMode::Auto,
            font_families: vec![
                fontdb::Family::Serif,
                fontdb::Family::SansSerif,
                fontdb::Family::Monospace,
            ],
            exact_fonts: Vec::new(),
            variant: FontVariant::Normal,
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
    /// The font this glyph was shaped with. `None` for glyphs on empty lines.
    pub font_key: Option<FontKey>,
    /// The font size used when rasterizing this glyph.
    pub font_size: f32,
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
