use std::collections::HashMap;
use std::sync::Arc;

use fontdue::{Font, FontSettings};
use rustybuzz::Face;

use crate::layout::RasterizedGlyph;
use crate::types::{FontKey, FontStyle, FontWeight};

/// Cached glyph bitmap (alpha mask).
struct CachedBitmap {
    metrics: RasterizedGlyph,
    alpha_mask: Vec<u8>,
}

/// Keeps font bytes alive while caching a parsed `rustybuzz::Face`.
///
/// `Face` borrows the underlying font data. This struct bundles them
/// together so that when the struct moves (e.g. inside a `HashMap`) the
/// heap‑allocated byte buffer stays at the same address.
struct CachedFace {
    _data: Vec<u8>,
    face: Face<'static>,
}

impl CachedFace {
    fn new(data: Vec<u8>, index: u32) -> Option<Self> {
        let face = Face::from_slice(&data, index)?;
        // Safety: `face` borrows from `data`, which is moved into the same
        // struct.  The `Vec`'s heap buffer address is stable across moves,
        // so the reference remains valid for as long as `CachedFace` lives.
        let face = unsafe { std::mem::transmute::<Face<'_>, Face<'static>>(face) };
        Some(CachedFace { _data: data, face })
    }

    fn face(&self) -> &Face<'_> {
        &self.face
    }
}

/// Manages font discovery, loading, and data storage.
///
/// Wraps a [`fontdb::Database`] and caches raw font data in memory so that
/// shapers and rasterizers can access it as byte slices.
pub struct FontSystem {
    pub(crate) db: fontdb::Database,
    pub(crate) font_data: HashMap<fontdb::ID, Vec<u8>>,
    pub(crate) parsed_fonts: HashMap<fontdb::ID, Font>,
    rasterized: HashMap<(fontdb::ID, u32, u32), RasterizedGlyph>,
    bitmap_cache: HashMap<(fontdb::ID, u32, u32), CachedBitmap>,
    face_cache: HashMap<fontdb::ID, CachedFace>,
}

impl FontSystem {
    /// Loads system fonts automatically.
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self {
            db,
            font_data: HashMap::new(),
            parsed_fonts: HashMap::new(),
            rasterized: HashMap::new(),
            bitmap_cache: HashMap::new(),
            face_cache: HashMap::new(),
        }
    }

    /// Creates a `FontSystem` with only the specified font data (no system fonts).
    ///
    /// Each element of `fonts_data` is a `Vec<u8>` of raw font file bytes (e.g., TTF, OTF).
    /// The data is cloned and stored internally.
    pub fn new_with_fonts(fonts_data: Vec<Vec<u8>>) -> Self {
        let mut db = fontdb::Database::new();
        let mut font_data = HashMap::new();
        for data in fonts_data {
            let source = fontdb::Source::Binary(Arc::new(data.clone()));
            let ids = db.load_font_source(source);
            for id in ids {
                font_data.insert(id, data.clone());
            }
        }
        Self {
            db,
            font_data,
            parsed_fonts: HashMap::new(),
            rasterized: HashMap::new(),
            bitmap_cache: HashMap::new(),
            face_cache: HashMap::new(),
        }
    }

    /// Returns a parsed [`Font`] for the given `font_key`, parsing and caching
    /// the font on first access. The parsed font is reused across all subsequent
    /// calls, avoiding repeated parsing of the same font file.
    pub fn get_or_parse_font(&mut self, font_key: FontKey) -> Option<&Font> {
        if self.parsed_fonts.contains_key(&font_key.0) {
            return Some(&self.parsed_fonts[&font_key.0]);
        }
        let data = self.get_font_data(font_key)?.to_vec();
        let font = Font::from_bytes(data, FontSettings::default()).ok()?;
        self.parsed_fonts.insert(font_key.0, font);
        Some(&self.parsed_fonts[&font_key.0])
    }

    /// Returns rasterized metrics for the given glyph, caching the result so
    /// the same glyph at the same font size is never rasterized twice.
    /// The font is looked up internally via [`get_or_parse_font`].
    pub(crate) fn get_or_rasterize(
        &mut self,
        font_key: FontKey,
        glyph_id: u32,
        font_size: f32,
    ) -> Option<&RasterizedGlyph> {
        let size_bits = font_size.to_bits();
        let key = (font_key.0, glyph_id, size_bits);
        if self.rasterized.contains_key(&key) {
            return Some(&self.rasterized[&key]);
        }
        let font = self.get_or_parse_font(font_key)?;
        let (metrics, _bitmap) = font.rasterize_indexed(glyph_id as u16, font_size);
        let rasterized = RasterizedGlyph {
            width: metrics.width as u32,
            height: metrics.height as u32,
            bearing_x: metrics.xmin,
            bearing_y: metrics.ymin + metrics.height as i32,
        };
        self.rasterized.insert(key, rasterized);
        Some(&self.rasterized[&key])
    }

    /// Returns rasterized metrics AND the alpha-mask bitmap for the given
    /// glyph, caching both so the same glyph at the same font size is never
    /// rasterized twice.
    ///
    /// Unlike [`get_or_rasterize`] (which returns a borrowed reference), this
    /// returns owned data so it can be used without fighting the borrow
    /// checker when also mutating `self` (e.g. rasterizing more glyphs).
    ///
    /// This is the rendering counterpart of [`get_or_rasterize`], which only
    /// caches metrics.
    pub fn get_or_rasterize_with_bitmap(
        &mut self,
        font_key: FontKey,
        glyph_id: u32,
        font_size: f32,
    ) -> Option<(RasterizedGlyph, Vec<u8>)> {
        let size_bits = font_size.to_bits();
        let key = (font_key.0, glyph_id, size_bits);
        if let Some(cached) = self.bitmap_cache.get(&key) {
            return Some((cached.metrics.clone(), cached.alpha_mask.clone()));
        }
        if !self.parsed_fonts.contains_key(&font_key.0) {
            let data = self.get_font_data(font_key)?.to_vec();
            let font = Font::from_bytes(data, FontSettings::default()).ok()?;
            self.parsed_fonts.insert(font_key.0, font);
        }
        let font = self.parsed_fonts.get(&font_key.0)?;
        let (metrics, alpha_mask) = font.rasterize_indexed(glyph_id as u16, font_size);
        let rasterized = RasterizedGlyph {
            width: metrics.width as u32,
            height: metrics.height as u32,
            bearing_x: metrics.xmin,
            bearing_y: metrics.ymin + metrics.height as i32,
        };
        self.bitmap_cache.insert(
            key,
            CachedBitmap {
                metrics: rasterized,
                alpha_mask,
            },
        );
        let cached = &self.bitmap_cache[&key];
        Some((cached.metrics.clone(), cached.alpha_mask.clone()))
    }

    /// Returns a cached `rustybuzz::Face` for the given font, creating and
    /// caching it on first access. The face is reused across all subsequent
    /// calls, avoiding repeated parsing of the font file tables.
    pub(crate) fn get_or_create_face(&mut self, key: FontKey) -> Option<&Face<'_>> {
        if !self.face_cache.contains_key(&key.0) {
            let data = self.get_font_data(key)?.to_vec();
            let cached = CachedFace::new(data, 0)?;
            self.face_cache.insert(key.0, cached);
        }
        Some(self.face_cache[&key.0].face())
    }

    /// Loads a font from raw byte data and returns the assigned keys.
    ///
    /// The data is cloned and stored internally so it remains available for
    /// shaping and rasterization.
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Vec<FontKey> {
        let source = fontdb::Source::Binary(Arc::new(data.clone()));
        let ids = self.db.load_font_source(source);
        ids.iter()
            .map(|id| {
                self.font_data.insert(*id, data.clone());
                FontKey(*id)
            })
            .collect()
    }

    /// Finds the best matching font for the given families, weight, and style.
    pub fn query(
        &self,
        families: &[fontdb::Family],
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<FontKey> {
        let query = fontdb::Query {
            families,
            weight: fontdb::Weight(weight.0),
            style: match style {
                FontStyle::Normal => fontdb::Style::Normal,
                FontStyle::Italic => fontdb::Style::Italic,
                FontStyle::Oblique => fontdb::Style::Oblique,
            },
            ..Default::default()
        };
        let id = self.db.query(&query)?;
        Some(FontKey(id))
    }

    /// Returns the keys of all currently loaded fonts.
    pub fn font_keys(&self) -> Vec<FontKey> {
        self.font_data.keys().copied().map(FontKey).collect()
    }

    /// Returns the raw font data for a given key.
    ///
    /// Data is loaded lazily from fontdb sources (file or binary) on first
    /// access and cached in memory for subsequent calls.
    pub fn get_font_data(&mut self, key: FontKey) -> Option<&[u8]> {
        if self.font_data.contains_key(&key.0) {
            return Some(self.font_data.get(&key.0).unwrap().as_slice());
        }
        let data = self
            .db
            .with_face_data(key.0, |font_data, _| font_data.to_vec())?;
        self.font_data.insert(key.0, data);
        self.font_data.get(&key.0).map(|v| v.as_slice())
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_non_empty_db() {
        let fs = FontSystem::new();
        assert!(fs.db.len() > 0, "expected at least one system font");
    }

    #[test]
    fn test_default_equals_new() {
        let a = FontSystem::new();
        let b = FontSystem::default();
        assert!(a.db.len() > 0);
        assert_eq!(a.db.len(), b.db.len());
    }
}
