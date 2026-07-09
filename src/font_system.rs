use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock, Mutex};

use fontdue::{Font, FontSettings};
use lru::LruCache;
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
/// Shares font data via `Arc` to avoid redundant copies across the
/// `font_data` instance map and the global font-data cache.
struct CachedFace {
    _data: Arc<Vec<u8>>,
    face: Face<'static>,
}

impl CachedFace {
    fn new(data: Arc<Vec<u8>>, index: u32) -> Option<Self> {
        let face = Face::from_slice(&data, index)?;
        // Safety: `face` borrows from `data`, which is moved into the same
        // struct.  The `Arc`'s heap buffer address is stable across moves,
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
/// default fonts via `FcFontMatch`, and to find the best font covering a
/// given character via `FcFontMatch` with a `CharSet`.
///
/// On Windows, uses DirectWrite (`IDWriteFontFallback::MapCharacters`) to
/// find the system-defined fallback font for a given character based on
/// the configured cascade list (e.g. Meiryo for Japanese, Nirmala UI for
/// Indic scripts, etc.).
///
/// On other platforms this is a no-op and fontdb's own resolution (from
/// fontconfig-XML parsing on Linux, or hardcoded defaults elsewhere) is
/// used as the fallback.
#[derive(Clone, Debug)]
pub struct PlatformFallback;

impl PlatformFallback {
    /// Resolves a CSS generic family name (e.g. `"sans-serif"`) to the
    /// system's actual default font family name and file path.
    pub fn resolve_generic_family(generic: &str) -> Option<(String, std::path::PathBuf)> {
        let result = imp::resolve_generic_family(generic);
        log::debug!(target: "orinium_text::platform", "resolve_generic_family({generic}) = {result:?}");
        result
    }

    /// Queries the OS-native font subsystem for the best font that covers
    /// `ch` and has a style compatible with the requested `weight`/`style`.
    ///
    /// Returns `(family_name, face_index)` on success.
    ///
    /// On Linux this uses fontconfig's `FcFontMatch` with a `CharSet`
    /// containing `ch`.  On Windows it uses DirectWrite's
    /// `IDWriteFontFallback::MapCharacters`.  Both APIs take the system
    /// font configuration into account, providing proper script-specific
    /// fallback (e.g. Meiryo on Windows, Noto Sans CJK on Linux).
    pub fn query_covering(
        ch: char,
        families: &[fontdb::Family],
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<(String, u32)> {
        let result = imp::query_covering(ch, families, weight, style);
        if let Some((ref name, _)) = result {
            log::info!(target: "orinium_text::platform", "font fallback for U+{:04X} ({ch}) → {name}", ch as u32);
        } else {
            log::trace!(target: "orinium_text::platform", "no platform fallback for U+{:04X} ({ch})", ch as u32);
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Platform-specific implementations
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod imp {
    use fontconfig::{CharSet, Fontconfig, Pattern};
    use std::path::PathBuf;

    pub fn resolve_generic_family(generic: &str) -> Option<(String, PathBuf)> {
        let fc = Fontconfig::new()?;
        let font = fc.find(generic, None).ok()?;
        Some((font.name, font.path))
    }

    pub fn query_covering(
        ch: char,
        _families: &[fontdb::Family],
        _weight: crate::types::FontWeight,
        _style: crate::types::FontStyle,
    ) -> Option<(String, u32)> {
        let fc = Fontconfig::new()?;
        let mut pat = Pattern::new(&fc).ok()?;
        let mut charset = CharSet::new(&fc).ok()?;
        charset.add_char(ch).ok()?;
        pat.add_charset(charset).ok()?;
        let matched = pat.font_match().ok()?;
        let name = matched.name().ok()?.to_owned();
        let index = matched.face_index().unwrap_or(0) as u32;
        Some((name, index))
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::borrow::Cow;
    use std::path::PathBuf;
    use winapi::um::dwrite::DWRITE_READING_DIRECTION_LEFT_TO_RIGHT;

    struct Source(String);
    impl dwrote::TextAnalysisSourceMethods for Source {
        fn get_locale_name(&self, _: u32) -> (Cow<'_, str>, u32) {
            (Cow::Borrowed(&self.0), u32::MAX)
        }
        fn get_paragraph_reading_direction(&self) -> i32 {
            DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
        }
    }

    pub fn resolve_generic_family(_generic: &str) -> Option<(String, PathBuf)> {
        None
    }

    pub fn query_covering(
        ch: char,
        families: &[fontdb::Family],
        weight: crate::types::FontWeight,
        style: crate::types::FontStyle,
    ) -> Option<(String, u32)> {
        let fallback = dwrote::FontFallback::get_system_fallback()?;
        let collection = dwrote::FontCollection::system();

        let utf16: Vec<u16> = ch.encode_utf16().collect();
        let source = dwrote::TextAnalysisSource::from_text(
            Box::new(Source(String::new())),
            Cow::Owned(utf16.clone()),
        );

        let dw_weight = match weight.0 {
            0..=49 => dwrote::FontWeight::Thin,
            50..=99 => dwrote::FontWeight::ExtraLight,
            100..=199 => dwrote::FontWeight::Light,
            200..=299 => dwrote::FontWeight::SemiLight,
            300..=399 => dwrote::FontWeight::Regular,
            400..=499 => dwrote::FontWeight::Medium,
            500..=599 => dwrote::FontWeight::SemiBold,
            600..=699 => dwrote::FontWeight::Bold,
            700..=799 => dwrote::FontWeight::ExtraBold,
            800..=899 => dwrote::FontWeight::Black,
            _ => dwrote::FontWeight::ExtraBlack,
        };
        let dw_style = match style {
            crate::types::FontStyle::Normal => dwrote::FontStyle::Normal,
            crate::types::FontStyle::Italic => dwrote::FontStyle::Italic,
            crate::types::FontStyle::Oblique => dwrote::FontStyle::Oblique,
        };

        let base_family = families.first().and_then(|f| match f {
            fontdb::Family::Name(name) => Some(name.as_ref()),
            _ => None,
        });
        let base_family_str = base_family.map(|s| s.to_owned());

        let result = fallback.map_characters(
            &source,
            0,
            utf16.len() as u32,
            &collection,
            base_family_str.as_deref(),
            dw_weight,
            dw_style,
            dwrote::FontStretch::Normal,
        );

        result.mapped_font.map(|font| (font.family_name(), 0))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod imp {
    use std::path::PathBuf;

    pub fn resolve_generic_family(_generic: &str) -> Option<(String, PathBuf)> {
        None
    }

    pub fn query_covering(
        _ch: char,
        _families: &[fontdb::Family],
        _weight: crate::types::FontWeight,
        _style: crate::types::FontStyle,
    ) -> Option<(String, u32)> {
        None
    }
}

/// If `data` is a TrueType Collection (starts with `ttcf`), extract the
/// individual font at `face_index`. Otherwise return `data` unchanged.
///
/// fontdue does not support TTC files natively.  This function slces out the
/// face at the given index and fixes up the table directory so that all table
/// offsets are relative to the start of the extracted data rather than
/// absolute within the original TTC file.
fn extract_ttc_face(data: &[u8], face_index: u32) -> Option<Vec<u8>> {
    if !data.starts_with(b"ttcf") {
        return Some(data.to_vec());
    }
    if data.len() < 12 {
        return None;
    }
    let num_fonts = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let idx = face_index as usize;
    if idx >= num_fonts as usize {
        return None;
    }
    let offset_pos = 12 + idx * 4;
    if offset_pos + 4 > data.len() {
        return None;
    }
    let ttc_offset = u32::from_be_bytes([
        data[offset_pos],
        data[offset_pos + 1],
        data[offset_pos + 2],
        data[offset_pos + 3],
    ]) as usize;
    let face_data = &data[ttc_offset..];

    // Fix up table directory offsets: they're absolute within the TTC file,
    // but need to be relative to the start of this face's data.
    if face_data.len() < 6 {
        return None;
    }
    let num_tables = u16::from_be_bytes([face_data[4], face_data[5]]) as usize;
    let mut result = face_data.to_vec();
    for i in 0..num_tables {
        let rec_pos = 12 + i * 16 + 8; // +8 to skip tag(4) + checksum(4)
        if rec_pos + 4 > result.len() {
            break;
        }
        let old_off = u32::from_be_bytes([
            result[rec_pos],
            result[rec_pos + 1],
            result[rec_pos + 2],
            result[rec_pos + 3],
        ]);
        let new_off = old_off.checked_sub(ttc_offset as u32)?;
        let bytes = new_off.to_be_bytes();
        result[rec_pos..rec_pos + 4].copy_from_slice(&bytes);
    }
    Some(result)
}

const BITMAP_CACHE_CAPACITY: usize = 5000;
const PARSED_FONTS_CAPACITY: usize = 64;

/// Manages font discovery, loading, and data storage.
///
/// Wraps a [`fontdb::Database`] and caches raw font data in memory so that
/// shapers and rasterizers can access it as byte slices.
///
/// Font data is stored as `Arc<Vec<u8>>` and shared across all caches
/// (`font_data`, `CachedFace`) to avoid redundant copies.
pub struct FontSystem {
    pub db: fontdb::Database,
    pub(crate) font_data: HashMap<fontdb::ID, Arc<Vec<u8>>>,
    pub(crate) parsed_fonts: LruCache<fontdb::ID, Arc<Font>>,
    bitmap_cache: LruCache<(fontdb::ID, u32, u32), CachedBitmap>,
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
            parsed_fonts: LruCache::new(NonZeroUsize::new(PARSED_FONTS_CAPACITY).unwrap()),
            bitmap_cache: LruCache::new(NonZeroUsize::new(BITMAP_CACHE_CAPACITY).unwrap()),
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
    ///
    /// Returns an `Arc` so the caller can hold a reference after the LRU cache
    /// potentially evicts the entry.
    pub fn get_or_parse_font(&mut self, font_key: FontKey) -> Option<Arc<Font>> {
        if let Some(font) = self.parsed_fonts.get(&font_key.0) {
            return Some(font.clone());
        }
        let data = self.get_font_data(font_key)?;
        let face_index = self.db.face(font_key.0)?.index;
        let face_data = extract_ttc_face(&data, face_index)?;
        let font = Font::from_bytes(face_data, FontSettings::default()).ok()?;
        let font = Arc::new(font);
        self.parsed_fonts.push(font_key.0, font.clone());
        Some(font)
    }

    /// Returns glyph dimensions (width, height, bearing_x, bearing_y) in
    /// pixels using `ttf_parser` for fast, lazy parsing.
    ///
    /// Unlike fontdue's `get_or_parse_font` + `metrics_indexed` (which may
    /// trigger expensive fontdue parsing of all glyph outlines), this uses
    /// parsing of the entire font, this uses `ttf_parser::Face::parse`
    /// which only validates the table directory and reads per-glyph data
    /// on demand.  Ideal for layout passes that don't need bitmaps.
    pub(crate) fn get_glyph_dimensions(
        &mut self,
        font_key: FontKey,
        glyph_id: u32,
        font_size: f32,
    ) -> Option<(f32, f32, f32, f32)> {
        let face_index = self.db.face(font_key.0)?.index;
        let data = self.get_font_data(font_key)?;

        let face = rustybuzz::ttf_parser::Face::parse(data.as_slice(), face_index).ok()?;
        let upem = face.units_per_em() as f32;
        let scale = font_size / upem;

        let glyph = rustybuzz::ttf_parser::GlyphId(glyph_id as u16);

        if let Some(bbox) = face.glyph_bounding_box(glyph) {
            Some((
                (bbox.x_max - bbox.x_min) as f32 * scale,
                (bbox.y_max - bbox.y_min) as f32 * scale,
                bbox.x_min as f32 * scale,
                bbox.y_max as f32 * scale,
            ))
        } else if let Some(advance) = face.glyph_hor_advance(glyph) {
            Some((advance as f32 * scale, 0.0, 0.0, 0.0))
        } else {
            None
        }
    }

    /// Returns rasterized metrics AND the alpha-mask bitmap for the given
    /// glyph, caching both so the same glyph at the same font size is never
    /// rasterized twice.
    ///
    /// Unlike the bitmap-free `get_glyph_dimensions` (which uses
    /// `ttf_parser` for fast lazy parsing), this calls fontdue to produce
    /// alpha masks — expensive for large CJK fonts.
    ///
    /// This is the rendering counterpart of `get_glyph_dimensions`, which
    /// only caches metrics.
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
        self.bitmap_cache.push(
            key,
            CachedBitmap {
                metrics: rasterized.clone(),
                alpha_mask: alpha_mask.clone(),
            },
        );
        Some((rasterized, alpha_mask))
    }

    /// Returns a cached `rustybuzz::Face` for the given font, creating and
    /// caching it on first access. The face is reused across all subsequent
    /// calls, avoiding repeated parsing of the font file tables.
    pub(crate) fn get_or_create_face(&mut self, key: FontKey) -> Option<&Face<'_>> {
        if !self.face_cache.contains_key(&key.0) {
            let data = self.get_font_data(key)?;

            let face_info = self.db.face(key.0)?;
            let cached = CachedFace::new(data, face_info.index)?;
            self.face_cache.insert(key.0, cached);
        }

        Some(self.face_cache[&key.0].face())
    }

    /// Loads a font from raw byte data and returns the assigned keys.
    ///
    /// The data is shared via `Arc` so multiple faces from the same source
    /// reuse the same heap allocation.
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Vec<FontKey> {
        let arc = Arc::new(data);
        let source = fontdb::Source::Binary(arc.clone());
        let ids = self.db.load_font_source(source);
        ids.iter()
            .map(|id| {
                self.font_data.insert(*id, arc.clone());
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
        if let Some(&key) = self.char_to_font.get(&ch) {
            log::debug!(target: "orinium_text::query_any", "U+{:04X} local-cache hit → {:?}", ch as u32, key);
            return Some(key);
        }

        // Global cache check: resolve by family name + face index.
        if let Some((family_name, idx)) = GLOBAL_CHAR_FONT.lock().unwrap().get(&ch).cloned() {
            log::debug!(target: "orinium_text::query_any", "U+{:04X} global-cache hit → {family_name} idx={idx}", ch as u32);
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

        // Try the OS-native font fallback API before falling back to
        // iterating every font in the database.  This gives correct
        // script-specific results (e.g. Meiryo on Windows for CJK,
        // Noto Sans Arabic on Linux for Arabic).
        log::debug!(target: "orinium_text::query_any", "U+{:04X} trying platform fallback", ch as u32);
        if let Some((family_name, idx)) = PlatformFallback::query_covering(
            ch,
            &[],
            crate::types::FontWeight(400),
            crate::types::FontStyle::Normal,
        ) {
            let candidate_ids: Vec<fontdb::ID> = self
                .db
                .faces()
                .filter(|f| f.index == idx && f.families.iter().any(|(n, _)| *n == family_name))
                .map(|f| f.id)
                .collect();
            for &id in &candidate_ids {
                let key = FontKey(id);
                if self.get_or_create_face(key).is_some() {
                    log::debug!(target: "orinium_text::query_any", "U+{:04X} platform fallback succeeded → {family_name} key={key:?}", ch as u32);
                    self.char_to_font.insert(ch, key);
                    GLOBAL_CHAR_FONT
                        .lock()
                        .unwrap()
                        .insert(ch, (family_name.clone(), idx));
                    return Some(key);
                }
            }
        } else {
            log::trace!(target: "orinium_text::query_any", "U+{:04X} platform fallback returned nothing", ch as u32);
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
                        if let Ok(ttfp) = rustybuzz::ttf_parser::Face::parse(&data, face_idx) {
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

            log::trace!(target: "orinium_text::query_any", "U+{:04X} scanning → id={id:?} family={family:?} style={style:?}", ch as u32);

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
        } else {
            log::warn!(target: "orinium_text::query_any", "U+{:04X} ({ch}) — no font found in entire system", ch as u32);
        }
        fallback_key
    }

    /// Returns the raw font data for a given key.
    ///
    /// Data is loaded lazily from fontdb sources (file or binary) on first
    /// access and cached (shared via `Arc`) for subsequent calls.
    pub fn get_font_data(&mut self, key: FontKey) -> Option<Arc<Vec<u8>>> {
        if let Some(data) = self.font_data.get(&key.0) {
            return Some(data.clone());
        }

        if let Some(data) = GLOBAL_FONT_DATA.lock().unwrap().get(&key.0).cloned() {
            self.font_data.insert(key.0, data.clone());
            return Some(data);
        }

        let data = self
            .db
            .with_face_data(key.0, |font_data, _| font_data.to_vec())?;

        let arc_data = Arc::new(data);
        GLOBAL_FONT_DATA
            .lock()
            .unwrap()
            .insert(key.0, arc_data.clone());
        self.font_data.insert(key.0, arc_data.clone());

        Some(arc_data)
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
