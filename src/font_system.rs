use std::collections::HashMap;
use std::sync::Arc;

use crate::types::{FontKey, FontStyle, FontWeight};

/// Manages font discovery, loading, and data storage.
///
/// Wraps a [`fontdb::Database`] and caches raw font data in memory so that
/// shapers and rasterizers can access it as byte slices.
pub struct FontSystem {
    pub(crate) db: fontdb::Database,
    pub(crate) font_data: HashMap<fontdb::ID, Vec<u8>>,
}

impl FontSystem {
    /// Loads system fonts automatically.
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self {
            db,
            font_data: HashMap::new(),
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
        Self { db, font_data }
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
