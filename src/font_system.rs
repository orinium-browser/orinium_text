use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use fontdue::{Font, FontSettings};
use rustybuzz::Face;

use crate::layout::RasterizedGlyph;
use crate::types::{FontKey, FontStyle, FontWeight};

/// Global cache: character → (font_family_name, face_index).
///
/// Shared across [`FontSystem`] instances so font-fallback results from
/// the measure phase are immediately available during buffer creation.
static GLOBAL_CHAR_FONT: LazyLock<Mutex<HashMap<char, (String, u32)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Global cache: fontdb ID → font bytes.
///
/// Shares loaded font bytes across [`FontSystem`] instances (e.g. measurement
/// and rendering) to prevent redundant disk reads and performance spikes.
static GLOBAL_FONT_DATA: LazyLock<Mutex<HashMap<fontdb::ID, Arc<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Global cache: (fontdb ID, character) → whether the font supports this character.
///
/// Shared across [`FontSystem`] instances to prevent repeated parsing of the same
/// font face's cmap table for fallback lookups.
static GLOBAL_CHAR_SUPPORT: LazyLock<Mutex<HashMap<(fontdb::ID, char), bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

/// Specifies the platform-specific font fallback strategy.
///
/// On Linux, uses the `fontconfig` C library to resolve generic CSS font
/// families (sans-serif, serif, monospace) to the user's actual system
/// default fonts via `FcFontMatch`.
///
/// On other platforms this is a no-op and fontdb's own resolution (from
/// fontconfig-XML parsing on Linux, or hardcoded defaults elsewhere) is
/// used as the fallback.
#[derive(Clone, Debug)]
pub struct PlatformFallback;

impl PlatformFallback {
    /// Resolves a CSS generic family name (e.g. `"sans-serif"`) to the
    /// system's actual default font family name and file path.
    ///
    /// Returns `None` when the platform cannot determine a default
    /// (e.g. macOS/Windows — not yet implemented).
    #[cfg(target_os = "linux")]
    pub fn resolve_generic_family(generic: &str) -> Option<(String, std::path::PathBuf)> {
        let fc = fontconfig::Fontconfig::new()?;
        let font = fc.find(generic, None).ok()?;
        Some((font.name, font.path))
    }

    /// Fallback for non-Linux platforms: no native API available yet.
    #[cfg(not(target_os = "linux"))]
    pub fn resolve_generic_family(_generic: &str) -> Option<(String, std::path::PathBuf)> {
        None
    }
}

/// Manages font discovery, loading, and data storage.
///
/// Wraps a [`fontdb::Database`] and caches raw font data in memory so that
/// shapers and rasterizers can access it as byte slices.
pub struct FontSystem {
    pub db: fontdb::Database,
    pub(crate) font_data: HashMap<fontdb::ID, Vec<u8>>,
    pub(crate) parsed_fonts: HashMap<fontdb::ID, Font>,
    rasterized: HashMap<(fontdb::ID, u32, u32), RasterizedGlyph>,
    bitmap_cache: HashMap<(fontdb::ID, u32, u32), CachedBitmap>,
    face_cache: HashMap<fontdb::ID, CachedFace>,
    /// Cache mapping characters to fonts that cover them.
    /// Populated by `query_any_covering` on first use.
    char_to_font: HashMap<char, FontKey>,
}

impl FontSystem {
    /// Loads system fonts automatically.
    pub fn new() -> Self {
        Self::new_with_fonts(core::iter::empty())
    }

    /// Creates a `FontSystem` with the given fonts, plus system fonts.
    ///
    /// Each item is a [`fontdb::Source`] (e.g. `Source::File` or `Source::Binary`).
    /// System fonts are loaded automatically before the supplied ones.
    ///
    /// Generic families (sans-serif, serif, monospace) are resolved by
    /// first trying the OS-native font resolution API (fontconfig on Linux,
    /// CoreText on macOS, DirectWrite on Windows), falling back to fontdb's
    /// internal fontconfig-XML parsing.
    pub fn new_with_fonts(fonts: impl IntoIterator<Item = fontdb::Source>) -> Self {
        let mut db = fontdb::Database::new();
        Self::load_fonts(&mut db, fonts.into_iter());
        Self::resolve_generic_families(&mut db);
        Self::new_with_locale_and_db_and_fallback(db, PlatformFallback)
    }

    /// Resolves generic CSS font families using OS-native API (B) with
    /// fontdb's own resolution as fallback (A).
    fn resolve_generic_families(db: &mut fontdb::Database) {
        let families: &[(&str, fn(&mut fontdb::Database, &str))] = &[
            ("sans-serif", |db, n| db.set_sans_serif_family(n)),
            ("serif", |db, n| db.set_serif_family(n)),
            ("monospace", |db, n| db.set_monospace_family(n)),
        ];
        for &(generic, setter) in families {
            if let Some((name, path)) = PlatformFallback::resolve_generic_family(generic) {
                if path.exists() {
                    db.load_font_source(fontdb::Source::File(path));
                }
                setter(db, &name);
            }
            // If resolve_generic_family returns None, fontdb's own resolution
            // (from fontconfig XML parsing in load_system_fonts) remains in
            // place — this is the (A) fallback.
        }
    }

    /// Builds a `FontSystem` from pre-configured parts.
    ///
    /// This is the lowest-level constructor — use [`new`](Self::new) or
    /// [`new_with_fonts`](Self::new_with_fonts) for convenience.
    pub fn new_with_locale_and_db_and_fallback(
        db: fontdb::Database,
        _fallback: PlatformFallback,
    ) -> Self {
        Self {
            db,
            font_data: HashMap::new(),
            parsed_fonts: HashMap::new(),
            rasterized: HashMap::new(),
            bitmap_cache: HashMap::new(),
            face_cache: HashMap::new(),
            char_to_font: HashMap::new(),
        }
    }

    /// Loads system fonts, then any user-supplied font sources.
    fn load_fonts(db: &mut fontdb::Database, fonts: impl Iterator<Item = fontdb::Source>) {
        db.load_system_fonts();
        for source in fonts {
            db.load_font_source(source);
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
        // Reuse bitmap cache if already populated (e.g. by render_text).
        if let Some(cached) = self.bitmap_cache.get(&key) {
            self.rasterized.insert(key, cached.metrics.clone());
            return Some(&self.rasterized[&key]);
        }
        let font = self.get_or_parse_font(font_key)?;
        let metrics = font.metrics_indexed(glyph_id as u16, font_size);
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
        let font = self.get_or_parse_font(font_key)?;
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
        // Also populate the metrics-only cache so callers that only
        // need metrics (layout) never rasterize again.
        self.rasterized
            .insert(key, self.bitmap_cache[&key].metrics.clone());
        let cached = &self.bitmap_cache[&key];
        Some((cached.metrics.clone(), cached.alpha_mask.clone()))
    }

    /// Returns a cached `rustybuzz::Face` for the given font, creating and
    /// caching it on first access. The face is reused across all subsequent
    /// calls, avoiding repeated parsing of the font file tables.
    pub(crate) fn get_or_create_face(&mut self, key: FontKey) -> Option<&Face<'_>> {
        if !self.face_cache.contains_key(&key.0) {
            let data = self.get_font_data(key)?.to_vec();

            let face_info = self.db.face(key.0)?;
            let cached = CachedFace::new(data, face_info.index)?;
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
        let db_weight = fontdb::Weight(weight.0);
        let db_style = match style {
            FontStyle::Normal => fontdb::Style::Normal,
            FontStyle::Italic => fontdb::Style::Italic,
            FontStyle::Oblique => fontdb::Style::Oblique,
        };
        let db_query = fontdb::Query {
            families,
            weight: db_weight,
            style: db_style,
            ..Default::default()
        };
        if let Some(id) = self.db.query(&db_query) {
            return Some(FontKey(id));
        }
        // When a generic family is requested but fontdb cannot match it
        // (e.g. on systems without fontconfig), return the first available
        // font as a fallback.
        let is_generic = families.iter().any(|f| {
            matches!(
                f,
                fontdb::Family::Serif
                    | fontdb::Family::SansSerif
                    | fontdb::Family::Monospace
                    | fontdb::Family::Cursive
                    | fontdb::Family::Fantasy
            )
        });
        if is_generic {
            // Prefer a Normal-style font over italic/oblique when the
            // requested generic family has no matching font installed.
            for face in self.db.faces() {
                if face.style == fontdb::Style::Normal {
                    return Some(FontKey(face.id));
                }
            }
            return self.db.faces().next().map(|f| FontKey(f.id));
        }
        None
    }

    /// Returns the keys of all currently loaded fonts.
    pub fn font_keys(&self) -> Vec<FontKey> {
        self.db.faces().map(|face| FontKey(face.id)).collect()
    }

    /// Returns the family name of the first available font face, if any.
    pub fn first_font_family_name(&self) -> Option<&str> {
        self.db
            .faces()
            .next()
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.as_str())
    }

    /// Returns a `FontKey` for every face in the database, regardless of
    /// whether the font data has been cached yet.
    pub fn all_font_keys(&self) -> Vec<FontKey> {
        self.db.faces().map(|face| FontKey(face.id)).collect()
    }

    /// Find any loaded font that covers the given character.
    ///
    /// This is a last-resort fallback: iterates all faces in the database,
    /// loads each font's data, checks via `ttf_parser::Face::glyph_index`
    /// (reads the cmap table, no shaping), and returns the first face that
    /// does NOT produce `.notdef` (glyph_id = 0).
    ///
    /// Results are cached:
    ///   - locally in `char_to_font` for instantaneous repeat lookups;
    ///   - globally in `GLOBAL_CHAR_FONT` so the same character never has to
    ///     be resolved again in a different `FontSystem` instance
    ///     (e.g. the platform text measurer and the GPU text renderer).
    /// Faces already cached in `face_cache` are skipped on subsequent calls
    /// since they were already tried through the normal fallback chain.
    pub fn query_any_covering(&mut self, ch: char) -> Option<FontKey> {
        // Fast path: return cached result if previously looked up.
        if let Some(&key) = self.char_to_font.get(&ch) {
            return Some(key);
        }

        // Global cache check: resolve by family name + face index.
        if let Some((family_name, idx)) = GLOBAL_CHAR_FONT.lock().unwrap().get(&ch).cloned() {
            for face_info in self.db.faces() {
                if face_info.index == idx
                    && face_info.families.iter().any(|(n, _)| *n == family_name)
                {
                    let key = FontKey(face_info.id);
                    self.char_to_font.insert(ch, key);
                    return Some(key);
                }
            }
        }

        let tried: std::collections::HashSet<fontdb::ID> =
            self.face_cache.keys().copied().collect();

        // Collect IDs first to avoid borrowing self.db and self simultaneously.
        let untried_ids: Vec<fontdb::ID> = self
            .db
            .faces()
            .map(|info| info.id)
            .filter(|id| !tried.contains(id))
            .collect();

        // Prefer Normal-style faces as a last-resort fallback.
        let mut fallback_key: Option<FontKey> = None;
        let mut fallback_family: Option<(String, u32)> = None;

        for &id in &untried_ids {
            let key = FontKey(id);

            // Collect face metadata first to avoid borrowing self.db and self
            // simultaneously when we later call self.get_font_data / get_or_create_face.
            let (face_idx, style, family) = match self.db.face(id) {
                Some(f) => (f.index, f.style, f.families.first().map(|(n, _)| n.clone())),
                None => continue,
            };

            let has_valid_glyph = {
                let cache_key = (id, ch);
                let cached_val = GLOBAL_CHAR_SUPPORT.lock().unwrap().get(&cache_key).copied();
                if let Some(supported) = cached_val {
                    supported
                } else {
                    let supported = if let Some(data) = self.get_font_data(key) {
                        if let Ok(ttfp) = rustybuzz::ttf_parser::Face::parse(data, face_idx) {
                            ttfp.glyph_index(ch)
                                .map_or(false, |gid| gid != rustybuzz::ttf_parser::GlyphId(0))
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    GLOBAL_CHAR_SUPPORT
                        .lock()
                        .unwrap()
                        .insert(cache_key, supported);
                    supported
                }
            };

            if !has_valid_glyph {
                continue;
            }

            // Found a covering glyph. Create the full rustybuzz Face for
            // later use, then cache and return.
            if self.get_or_create_face(key).is_none() {
                continue;
            }

            if style == fontdb::Style::Normal {
                self.char_to_font.insert(ch, key);
                if let Some(ref name) = family {
                    GLOBAL_CHAR_FONT
                        .lock()
                        .unwrap()
                        .insert(ch, (name.clone(), face_idx));
                }
                return Some(key);
            }

            if fallback_key.is_none() {
                fallback_key = Some(key);
                fallback_family = family.map(|name| (name, face_idx));
            }
        }

        if let Some(key) = fallback_key {
            self.char_to_font.insert(ch, key);
            if let Some((name, idx)) = fallback_family {
                GLOBAL_CHAR_FONT.lock().unwrap().insert(ch, (name, idx));
            }
        }
        fallback_key
    }

    /// Returns the raw font data for a given key.
    ///
    /// Data is loaded lazily from fontdb sources (file or binary) on first
    /// access and cached in memory for subsequent calls.
    pub fn get_font_data(&mut self, key: FontKey) -> Option<&[u8]> {
        if self.font_data.contains_key(&key.0) {
            return Some(self.font_data.get(&key.0).unwrap().as_slice());
        }

        if let Some(data) = GLOBAL_FONT_DATA.lock().unwrap().get(&key.0).cloned() {
            self.font_data.insert(key.0, (*data).clone());
            return Some(self.font_data.get(&key.0).unwrap().as_slice());
        }

        let data = self
            .db
            .with_face_data(key.0, |font_data, _| font_data.to_vec())?;

        let arc_data = Arc::new(data);
        GLOBAL_FONT_DATA
            .lock()
            .unwrap()
            .insert(key.0, arc_data.clone());
        self.font_data.insert(key.0, (*arc_data).clone());

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
