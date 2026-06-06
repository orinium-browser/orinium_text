use std::collections::HashMap;
use std::sync::Arc;

use fontdue::{Font, FontSettings};
use rustybuzz::{Direction as RbDirection, Face, UnicodeBuffer};
use unicode_bidi::{BidiInfo, Level};
use unicode_linebreak::linebreaks;

use crate::FontSystem;
use crate::types::{BidiMode, FontKey, FontStyle, FontWeight, LayoutGlyph, LayoutLine, TextLayout, TextStyle};

struct GlyphPosition {
    glyph_id: u32,
    x_advance: f32,
    y_advance: f32,
    x_offset: f32,
    y_offset: f32,
    cluster: u32,
}

struct ShapedRun {
    glyphs: Vec<GlyphPosition>,
    font_key: FontKey,
    font_data: Vec<u8>,
    direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    fn is_rtl(&self) -> bool {
        matches!(self, Direction::Rtl)
    }
}

struct RasterizedGlyph {
    width: u32,
    height: u32,
    bearing_x: i32,
    bearing_y: i32,
}

fn rasterize(font_data: &[u8], glyph_index: u16, font_size: f32) -> Option<RasterizedGlyph> {
    let font = Font::from_bytes(font_data, FontSettings::default()).ok()?;
    let (metrics, _bitmap) = font.rasterize_indexed(glyph_index, font_size);
    Some(RasterizedGlyph {
        width: metrics.width as u32,
        height: metrics.height as u32,
        bearing_x: metrics.xmin,
        bearing_y: metrics.ymin,
    })
}

fn shape_text(
    font_data: &[u8],
    face_index: u32,
    text: &str,
    font_size: f32,
    direction: Direction,
) -> Vec<GlyphPosition> {
    let face = match Face::from_slice(font_data, face_index) {
        Some(face) => face,
        None => return Vec::new(),
    };

    let upem = face.units_per_em() as f32;
    let scale = font_size / upem / 64.0;

    let rb_direction = match direction {
        Direction::Ltr => RbDirection::LeftToRight,
        Direction::Rtl => RbDirection::RightToLeft,
    };

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(rb_direction);

    let glyph_buffer = rustybuzz::shape(&face, &[], buffer);
    let glyph_infos = glyph_buffer.glyph_infos();
    let glyph_positions = glyph_buffer.glyph_positions();

    glyph_infos
        .iter()
        .zip(glyph_positions.iter())
        .map(|(info, pos)| GlyphPosition {
            glyph_id: info.glyph_id as u32,
            x_advance: pos.x_advance as f32 * scale,
            y_advance: pos.y_advance as f32 * scale,
            x_offset: pos.x_offset as f32 * scale,
            y_offset: pos.y_offset as f32 * scale,
            cluster: info.cluster as u32,
        })
        .collect()
}

fn find_font(
    font_system: &FontSystem,
    families: &[fontdb::Family],
    weight: FontWeight,
    style: FontStyle,
) -> Option<(FontKey, Vec<u8>)> {
    if families.is_empty() {
        return None;
    }
    let key = font_system.query(families, weight, style)?;
    let data = font_system.get_font_data(key)?;
    Some((key, data.to_vec()))
}

/// Finds contiguous byte ranges with/without coverage, based on `.notdef` glyphs.
///
/// Returns `(start, end, covered)` triples aligned to the glyph cluster
/// boundaries from `glyphs`.
fn coverage_ranges(text: &str, glyphs: &[GlyphPosition]) -> Vec<(usize, usize, bool)> {
    let mut clusters: Vec<(usize, bool)> = glyphs
        .iter()
        .map(|g| (g.cluster as usize, g.glyph_id != 0))
        .collect();
    clusters.sort_by_key(|(c, _)| *c);
    clusters.dedup_by_key(|(c, _)| *c);

    if clusters.is_empty() {
        return vec![(0, text.len(), false)];
    }

    let mut ranges = Vec::new();
    let mut start = clusters[0].0;
    let mut covered = clusters[0].1;

    for &(cluster, is_covered) in &clusters[1..] {
        if is_covered != covered {
            ranges.push((start, cluster, covered));
            start = cluster;
            covered = is_covered;
        }
    }
    ranges.push((start, text.len(), covered));
    ranges
}

/// Shapes `text` with the given font, splitting on `.notdef` if needed.
///
/// `try_next` is called for any byte-range where the font produced `.notdef`
/// glyphs, allowing the caller to supply a fallback font.
fn shape_with_font(
    text: &str,
    run_start: usize,
    font_key: FontKey,
    font_data: &[u8],
    font_system: &mut FontSystem,
    font_size: f32,
    try_next: &mut dyn FnMut(&str, usize, &mut FontSystem) -> Vec<ShapedRun>,
    direction: Direction,
) -> Vec<ShapedRun> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut glyphs = shape_text(font_data, 0, text, font_size, direction);
    for g in &mut glyphs {
        g.cluster += run_start as u32;
    }

    let has_notdef = glyphs.iter().any(|g| g.glyph_id == 0);

    if !has_notdef {
        return vec![ShapedRun {
            glyphs,
            font_key,
            font_data: font_data.to_vec(),
            direction,
        }];
    }

    let ranges = coverage_ranges(text, &glyphs);
    let mut result = Vec::new();

    for (start, end, covered) in ranges {
        if start >= end {
            continue;
        }
        let sub_text = &text[start..end];
        if covered {
            let sub_glyphs = shape_text(font_data, 0, sub_text, font_size, direction);
            let mut adjusted = sub_glyphs;
            for g in &mut adjusted {
                g.cluster += (run_start + start) as u32;
            }
            result.push(ShapedRun {
                glyphs: adjusted,
                font_key,
                font_data: font_data.to_vec(),
                direction,
            });
        } else {
            let fallback = try_next(sub_text, run_start + start, font_system);
            result.extend(fallback);
        }
    }

    result
}

/// Shapes `text` through the full fallback chain: first try each
/// `exact_fonts` (direct `FontKey` lookup), then each `font_families`
/// (name-query), splitting on `.notdef` at every level.
fn shape_with_fallback(
    text: &str,
    run_start: usize,
    font_system: &mut FontSystem,
    weight: FontWeight,
    style: FontStyle,
    font_size: f32,
    exact_fonts: &[FontKey],
    families: &[fontdb::Family],
    direction: Direction,
) -> Vec<ShapedRun> {
    if text.is_empty() {
        return Vec::new();
    }

    // Phase 1: try exact fonts by FontKey (no family-name query)
    if !exact_fonts.is_empty() {
        let font_key = exact_fonts[0];
        let font_data = match font_system.get_font_data(font_key) {
            Some(d) => d.to_vec(),
            None => {
                return shape_with_fallback(
                    text, run_start, font_system, weight, style, font_size,
                    &exact_fonts[1..], families, direction,
                );
            }
        };

        return shape_with_font(
            text, run_start, font_key, &font_data, font_system, font_size,
            &mut |sub, offset, fs| {
                shape_with_fallback(
                    sub, offset, fs, weight, style, font_size,
                    &exact_fonts[1..], families, direction,
                )
            },
            direction,
        );
    }

    // Phase 2: try font families (query by name)
    if families.is_empty() {
        return Vec::new();
    }

    let (font_key, font_data) = match find_font(font_system, &families[..1], weight, style) {
        Some(kd) => kd,
        None => {
            return shape_with_fallback(
                text, run_start, font_system, weight, style, font_size,
                exact_fonts, &families[1..], direction,
            );
        }
    };

    shape_with_font(
        text, run_start, font_key, &font_data, font_system, font_size,
        &mut |sub, offset, fs| {
            shape_with_fallback(
                sub, offset, fs, weight, style, font_size,
                exact_fonts, &families[1..], direction,
            )
        },
        direction,
    )
}

struct GlyphCacheEntry {
    rasterized: RasterizedGlyph,
}

/// Cache keyed by `(FontKey, glyph_id, font_size_bits)`. Holds [`Arc`] so that
/// the same rasterized glyph can be shared across lines without copying pixels.
#[derive(Default)]
struct GlyphCache {
    entries: HashMap<(FontKey, u32, u32), Arc<GlyphCacheEntry>>,
}

impl GlyphCache {
    fn get_or_rasterize(
        &mut self,
        font_key: FontKey,
        font_data: &[u8],
        glyph_id: u32,
        font_size: f32,
    ) -> Option<Arc<GlyphCacheEntry>> {
        let size_bits = font_size.to_bits();
        let key = (font_key, glyph_id, size_bits);
        if let Some(entry) = self.entries.get(&key) {
            return Some(Arc::clone(entry));
        }
        let rasterized = rasterize(font_data, glyph_id as u16, font_size)?;
        let entry = Arc::new(GlyphCacheEntry { rasterized });
        self.entries.insert(key, Arc::clone(&entry));
        Some(entry)
    }
}

/// Lays out text into positioned glyph runs.
///
/// Combines font matching, bidirectional text resolution, Unicode line breaking,
/// shaping via rustybuzz, and glyph rasterization via fontdue into a single
/// `layout_text` call.
pub struct TextLayouter {
    glyph_cache: GlyphCache,
}

impl TextLayouter {
    /// Creates a new `TextLayouter` with an empty glyph cache.
    pub fn new() -> Self {
        Self {
            glyph_cache: GlyphCache::default(),
        }
    }

    /// Produces a fully laid-out `TextLayout` for the given text.
    ///
    /// Internally runs bidi resolution, line breaking, shaping, and
    /// rasterization. Glyph bitmaps are cached across calls to avoid
    /// re-rasterizing identical characters at the same size.
    pub fn layout_text(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        style: &TextStyle<'_>,
        max_width: Option<f32>,
    ) -> TextLayout {
        if text.is_empty() || font_system.font_data.is_empty() {
            return TextLayout {
                lines: Vec::new(),
                width: 0.0,
                height: 0.0,
            };
        }

        let bidi_level = match style.bidi_mode {
            BidiMode::Auto => None,
            BidiMode::Ltr => Some(Level::ltr()),
            BidiMode::Rtl => Some(Level::rtl()),
        };
        let bidi_info = BidiInfo::new(text, bidi_level);
        let line_height = style.font_size * style.line_height;
        let mut all_lines: Vec<LayoutLine> = Vec::new();
        let mut current_y = 0.0_f32;
        let mut max_line_width = 0.0_f32;

        for para_idx in 0..bidi_info.paragraphs.len() {
            let (para_range, para_level) = match bidi_info.paragraphs.get(para_idx) {
                Some(p) => (p.range.clone(), p.level.number()),
                None => continue,
            };

            let para_text = &text[para_range.clone()];

            if para_text.is_empty() {
                all_lines.push(LayoutLine {
                    glyphs: Vec::new(),
                    x: 0.0,
                    y: current_y,
                    width: 0.0,
                    height: line_height,
                    baseline: style.font_size * 0.8,
                });
                current_y += line_height;
                continue;
            }

            let levels: Vec<u8> = bidi_info.levels[para_range.clone()]
                .iter()
                .map(|l| l.number())
                .collect();
            let base_direction = if para_level & 1 == 1 {
                Direction::Rtl
            } else {
                Direction::Ltr
            };

            let runs = build_visual_runs(para_text, base_direction, &levels);

            let mut shaped_runs: Vec<(ShapedRun, Direction)> = Vec::new();
            for run in &runs {
                let run_text = &para_text[run.start..run.end];
                let fallback_runs = shape_with_fallback(
                    run_text,
                    run.start,
                    font_system,
                    style.font_weight,
                    style.font_style,
                    style.font_size,
                    &style.exact_fonts,
                    &style.font_families,
                    run.direction,
                );
                for sr in fallback_runs {
                    let dir = sr.direction;
                    shaped_runs.push((sr, dir));
                }
            }

            let mut break_ops: Vec<usize> = linebreaks(para_text).map(|(pos, _)| pos).collect();
            break_ops.sort();
            break_ops.dedup();
            let wrap_width = max_width.unwrap_or(f32::MAX);

            let line_ranges =
                build_line_ranges(para_text.len(), &shaped_runs, &break_ops, wrap_width);

            for (line_start, line_end) in line_ranges {
                let mut line_glyphs: Vec<ShapedRun> = Vec::new();
                let line_byte_start = para_range.start + line_start;

                for (run, dir) in &shaped_runs {
                    let run_byte_start = para_range.start;
                    for g in &run.glyphs {
                        let abs_cluster = run_byte_start + g.cluster as usize;
                        if abs_cluster >= line_byte_start
                            && abs_cluster < line_byte_start + (line_end - line_start)
                        {
                            let last = line_glyphs.last_mut();
                            if let Some(last) = last {
                                if last.font_key == run.font_key && last.direction == *dir {
                                    last.glyphs.push(GlyphPosition {
                                        glyph_id: g.glyph_id,
                                        x_advance: g.x_advance,
                                        y_advance: g.y_advance,
                                        x_offset: g.x_offset,
                                        y_offset: g.y_offset,
                                        cluster: g.cluster,
                                    });
                                    continue;
                                }
                            }
                            line_glyphs.push(ShapedRun {
                                glyphs: vec![GlyphPosition {
                                    glyph_id: g.glyph_id,
                                    x_advance: g.x_advance,
                                    y_advance: g.y_advance,
                                    x_offset: g.x_offset,
                                    y_offset: g.y_offset,
                                    cluster: g.cluster,
                                }],
                                font_key: run.font_key,
                                font_data: run.font_data.clone(),
                                direction: *dir,
                            });
                        }
                    }
                }

                let mut line_width = 0.0_f32;
                let mut max_ascent = 0.0_f32;
                let mut max_descent = 0.0_f32;
                let mut positioned: Vec<(f32, u32, f32, f32, f32)> = Vec::new();

                for run in &line_glyphs {
                    for g in &run.glyphs {
                        let cache_entry = self.glyph_cache.get_or_rasterize(
                            run.font_key,
                            &run.font_data,
                            g.glyph_id,
                            style.font_size,
                        );
                        let (w, h, bx, by) = match cache_entry {
                            Some(ref entry) => (
                                entry.rasterized.width as f32,
                                entry.rasterized.height as f32,
                                entry.rasterized.bearing_x as f32,
                                entry.rasterized.bearing_y as f32,
                            ),
                            None => (0.0, 0.0, 0.0, 0.0),
                        };

                        let ascent = by;
                        let descent = (h - by).max(0.0);
                        max_ascent = max_ascent.max(ascent);
                        max_descent = max_descent.max(descent);

                        let x_pos = line_width + g.x_offset + bx;
                        let y_pos = ascent - by + g.y_offset;

                        positioned.push((x_pos, g.glyph_id, w, h, y_pos));
                        line_width += g.x_advance;
                    }
                }

                if line_width > max_line_width {
                    max_line_width = line_width;
                }

                let line_height_px = max_ascent + max_descent;
                let baseline = max_ascent;

                let positioned_glyphs: Vec<LayoutGlyph> = positioned
                    .into_iter()
                    .map(|(x, gid, w, h, y)| LayoutGlyph {
                        glyph_id: gid,
                        x,
                        y: current_y + y,
                        width: w,
                        height: h,
                        color: style.color,
                    })
                    .collect();

                all_lines.push(LayoutLine {
                    glyphs: positioned_glyphs,
                    x: 0.0,
                    y: current_y,
                    width: line_width,
                    height: line_height_px.max(line_height),
                    baseline,
                });

                current_y += line_height_px.max(line_height);
            }

            if runs.is_empty() {
                current_y += line_height;
            }
        }

        TextLayout {
            lines: all_lines,
            width: max_line_width,
            height: current_y,
        }
    }
}

impl Default for TextLayouter {
    fn default() -> Self {
        Self::new()
    }
}

struct VisualRun {
    start: usize,
    end: usize,
    direction: Direction,
}

/// Splits a paragraph into visual runs based on bidi levels.
///
/// Each run is a contiguous span of text that shares the same resolved
/// direction (LTR or RTL). Adjacent runs with the same direction are merged.
fn build_visual_runs(
    text: &str,
    base_direction: Direction,
    levels: &[u8],
) -> Vec<VisualRun> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut runs: Vec<VisualRun> = Vec::new();
    let mut run_start = 0;
    let mut current_dir = if base_direction.is_rtl() {
        Direction::Rtl
    } else {
        Direction::Ltr
    };

    for (i, _ch) in text.char_indices() {
        let level = levels.get(i).copied().unwrap_or(0);
        let dir = if level & 1 == 1 {
            Direction::Rtl
        } else {
            Direction::Ltr
        };

        if dir != current_dir && i > run_start {
            runs.push(VisualRun {
                start: run_start,
                end: i,
                direction: current_dir,
            });
            run_start = i;
            current_dir = dir;
        }
    }

    if run_start < text.len() {
        runs.push(VisualRun {
            start: run_start,
            end: text.len(),
            direction: current_dir,
        });
    }

    runs
}

/// Groups shaped glyphs into line ranges based on width and break opportunities.
///
/// Returns `(start, end)` byte-index pairs into the original text. Each pair
/// represents one line's worth of content after soft-wrapping at word boundaries.
fn build_line_ranges(
    text_len: usize,
    shaped_runs: &[(ShapedRun, Direction)],
    break_ops: &[usize],
    max_width: f32,
) -> Vec<(usize, usize)> {
    if text_len == 0 {
        return Vec::new();
    }

    if max_width >= f32::MAX || max_width <= 0.0 {
        return vec![(0, text_len)];
    }

    let char_advances: Vec<f32> = {
        let mut advances = Vec::with_capacity(text_len);
        for (run, _) in shaped_runs {
            for g in &run.glyphs {
                advances.push(g.x_advance);
            }
        }
        if advances.is_empty() {
            return vec![(0, text_len)];
        }
        advances
    };

    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0_usize;
    let mut last_break = 0_usize;
    let mut line_width = 0.0_f32;

    for (i, &advance) in char_advances.iter().enumerate() {
        line_width += advance;
        if line_width > max_width {
            let break_at = if last_break > line_start {
                last_break
            } else {
                i
            };
            lines.push((line_start, break_at));
            line_start = break_at;
            line_width = 0.0;
            last_break = break_at;
            if break_at < i {
                line_width += advance;
            }
        }

        let is_break = break_ops.contains(&(i + 1));
        if is_break && i + 1 > line_start {
            last_break = i + 1;
        }
    }

    if line_start < char_advances.len() {
        lines.push((line_start, char_advances.len()));
    }

    if lines.is_empty() {
        lines.push((0, char_advances.len()));
    }

    lines
}
