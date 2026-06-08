use rustybuzz::{Direction as RbDirection, Face, UnicodeBuffer};
use unicode_bidi::{BidiInfo, Level};
use unicode_linebreak::linebreaks;

use crate::FontSystem;
use crate::types::{
    BidiMode, FontKey, FontStyle, FontWeight, LayoutGlyph, LayoutLine, TextLayout, TextStyle,
};

#[derive(Debug, Clone)]
struct GlyphPosition {
    glyph_id: u32,
    x_advance: f32,
    y_advance: f32,
    x_offset: f32,
    y_offset: f32,
    cluster: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ShapedRun {
    glyphs: Vec<GlyphPosition>,
    font_key: FontKey,
    direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    fn is_rtl(&self) -> bool {
        matches!(self, Direction::Rtl)
    }
}

/// Metrics for a rasterized glyph: dimensions and pixel-space bearing.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    pub bearing_x: i32,
    pub bearing_y: i32,
}

fn shape_text(
    face: &Face,
    text: &str,
    font_size: f32,
    direction: Direction,
) -> Vec<GlyphPosition> {
    let upem = face.units_per_em() as f32;
    let scale = font_size / upem;

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
    font_system: &mut FontSystem,
    families: &[fontdb::Family],
    weight: FontWeight,
    style: FontStyle,
) -> Option<FontKey> {
    if families.is_empty() {
        return None;
    }
    let key = font_system.query(families, weight, style)?;
    font_system.get_font_data(key)?;
    Some(key)
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
    font_system: &mut FontSystem,
    font_size: f32,
    try_next: &mut dyn FnMut(&str, usize, &mut FontSystem) -> Vec<ShapedRun>,
    direction: Direction,
) -> Vec<ShapedRun> {
    if text.is_empty() {
        return Vec::new();
    }

    // Scoped so the `face` borrow from font_system is released before
    // any `try_next` call (which also needs font_system).
    let glyphs = match font_system.get_or_create_face(font_key) {
        Some(face) => shape_text(face, text, font_size, direction),
        None => return Vec::new(),
    };
    let has_notdef = glyphs.iter().any(|g| g.glyph_id == 0);

    if !has_notdef {
        let mut adjusted = glyphs;
        for g in &mut adjusted {
            g.cluster += run_start as u32;
        }
        return vec![ShapedRun {
            glyphs: adjusted,
            font_key,
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
            // Re-acquire face from cache; the initial borrow was released above.
            let face = match font_system.get_or_create_face(font_key) {
                Some(f) => f,
                None => continue,
            };
            let sub_glyphs = shape_text(face, sub_text, font_size, direction);
            let mut adjusted = sub_glyphs;
            for g in &mut adjusted {
                g.cluster += (run_start + start) as u32;
            }
            result.push(ShapedRun {
                glyphs: adjusted,
                font_key,
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
        if font_system.get_font_data(font_key).is_none() {
            return shape_with_fallback(
                text,
                run_start,
                font_system,
                weight,
                style,
                font_size,
                &exact_fonts[1..],
                families,
                direction,
            );
        }

        return shape_with_font(
            text,
            run_start,
            font_key,
            font_system,
            font_size,
            &mut |sub, offset, fs| {
                shape_with_fallback(
                    sub,
                    offset,
                    fs,
                    weight,
                    style,
                    font_size,
                    &exact_fonts[1..],
                    families,
                    direction,
                )
            },
            direction,
        );
    }

    // Phase 2: try font families (query by name)
    if families.is_empty() {
        return Vec::new();
    }

    let font_key = match find_font(font_system, &families[..1], weight, style) {
        Some(key) => key,
        None => {
            return shape_with_fallback(
                text,
                run_start,
                font_system,
                weight,
                style,
                font_size,
                exact_fonts,
                &families[1..],
                direction,
            );
        }
    };

    shape_with_font(
        text,
        run_start,
        font_key,
        font_system,
        font_size,
        &mut |sub, offset, fs| {
            shape_with_fallback(
                sub,
                offset,
                fs,
                weight,
                style,
                font_size,
                exact_fonts,
                &families[1..],
                direction,
            )
        },
        direction,
    )
}

/// A single layout fragment — the atomic unit the external layout engine
/// works with. Each fragment corresponds to one glyph cluster and carries
/// its size and line-break eligibility.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// Byte offset of this fragment's first character in the original text.
    pub cluster: usize,
    /// X position in a single-line (unwrapped) layout.
    pub x: f32,
    /// Advance width of this fragment in pixels.
    pub width: f32,
    /// Whether a line break is permitted immediately after this fragment.
    pub break_after: bool,
}

/// The result of shaping text without line-breaking or glyph positioning.
///
/// An external layout engine iterates [`fragments`](ShapedText::fragments),
/// accumulates [`Fragment::width`] values, and decides where to break
/// (respecting [`Fragment::break_after`]).  The resulting byte ranges are
/// passed to [`TextLayouter::layout_lines`].
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Flat list of fragments in visual order.
    pub fragments: Vec<Fragment>,
    /// Per-paragraph internal shape data (used by `layout_lines`).
    pub(crate) paras: Vec<ParaShapedData>,
}

/// Per-paragraph internal shaping data for line positioning.
#[derive(Debug, Clone)]
pub(crate) struct ParaShapedData {
    pub(crate) offset: usize,
    pub(crate) len: usize,
    pub(crate) shaped_runs: Vec<(ShapedRun, Direction)>,
}

/// Lays out text into positioned glyph runs.
///
/// Combines font matching, bidirectional text resolution, Unicode line breaking,
/// shaping via rustybuzz, and glyph rasterization via fontdue into a single
/// `layout_text` call.
pub struct TextLayouter;

impl TextLayouter {
    /// Creates a new `TextLayouter`.
    pub fn new() -> Self {
        Self
    }

    /// Shapes the text without line-breaking or glyph positioning.
    ///
    /// Returns a [`ShapedText`] containing glyph runs and break opportunities
    /// that an external layout engine can use to determine line breaks.
    pub fn shape_text(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        style: &TextStyle<'_>,
    ) -> ShapedText {
        if text.is_empty() || font_system.db.len() == 0 {
            return ShapedText {
                fragments: Vec::new(),
                paras: Vec::new(),
            };
        }

        let bidi_level = match style.bidi_mode {
            BidiMode::Auto => None,
            BidiMode::Ltr => Some(Level::ltr()),
            BidiMode::Rtl => Some(Level::rtl()),
        };
        let bidi_info = BidiInfo::new(text, bidi_level);

        let mut all_fragments: Vec<Fragment> = Vec::new();
        let mut paras: Vec<ParaShapedData> = Vec::new();

        for para_idx in 0..bidi_info.paragraphs.len() {
            let (para_range, para_level) = match bidi_info.paragraphs.get(para_idx) {
                Some(p) => (p.range.clone(), p.level.number()),
                None => continue,
            };

            let para_text = &text[para_range.clone()];

            if para_text.is_empty() {
                paras.push(ParaShapedData {
                    offset: para_range.start,
                    len: 0,
                    shaped_runs: Vec::new(),
                });
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

            let break_ops: Vec<usize> = linebreaks(para_text)
                .map(|(pos, _)| para_range.start + pos)
                .collect();

            // Build fragments for this paragraph
            let para_end = para_range.start + para_text.len();
            let mut para_x = 0.0_f32;
            let mut para_frags: Vec<Fragment> = Vec::new();

            for (run, _) in &shaped_runs {
                for g in &run.glyphs {
                    let cluster = para_range.start + g.cluster as usize;
                    para_frags.push(Fragment {
                        cluster,
                        x: para_x,
                        width: g.x_advance,
                        break_after: false,
                    });
                    para_x += g.x_advance;
                }
            }

            // Mark break opportunities using cluster boundaries
            for i in 0..para_frags.len() {
                let next_cluster = para_frags.get(i + 1).map(|f| f.cluster).unwrap_or(para_end);
                if break_ops.contains(&next_cluster) {
                    para_frags[i].break_after = true;
                }
            }

            all_fragments.extend(para_frags);
            paras.push(ParaShapedData {
                offset: para_range.start,
                len: para_text.len(),
                shaped_runs,
            });
        }

        ShapedText {
            fragments: all_fragments,
            paras,
        }
    }

    /// Positions glyphs into lines according to the provided line ranges.
    ///
    /// `line_ranges` must be `(start_byte, end_byte)` pairs into the original
    /// text, typically produced by an external layout engine.
    pub fn layout_lines(
        &mut self,
        font_system: &mut FontSystem,
        shaped: &ShapedText,
        line_ranges: &[(usize, usize)],
        style: &TextStyle<'_>,
    ) -> TextLayout {
        if line_ranges.is_empty() {
            return TextLayout {
                lines: Vec::new(),
                width: 0.0,
                height: 0.0,
            };
        }

        let line_height = style.font_size * style.line_height;
        let mut all_lines: Vec<LayoutLine> = Vec::new();
        let mut current_y = 0.0_f32;
        let mut max_line_width = 0.0_f32;

        for &(line_start, line_end) in line_ranges {
            // Find the paragraph containing this line
            let para = match shaped
                .paras
                .iter()
                .find(|p| {
                    p.offset <= line_start
                        && (if p.len == 0 {
                            p.offset == line_start
                        } else {
                            line_start < p.offset + p.len
                        })
                })
            {
                Some(p) => p,
                None => {
                    all_lines.push(LayoutLine {
                        glyphs: Vec::new(),
                        x: 0.0,
                        y: current_y,
                        width: 0.0,
                        height: line_height,
                        baseline: 0.0,
                    });
                    current_y += line_height;
                    continue;
                }
            };

            // Filter glyphs for this line range
            let mut line_glyphs: Vec<ShapedRun> = Vec::new();
            for (run, dir) in &para.shaped_runs {
                let run_byte_start = para.offset;
                for g in &run.glyphs {
                    let abs_cluster = run_byte_start + g.cluster as usize;
                    if abs_cluster >= line_start && abs_cluster < line_end {
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
                            direction: *dir,
                        });
                    }
                }
            }

            // Position glyphs
            let mut line_width = 0.0_f32;
            let mut max_ascent = 0.0_f32;
            let mut max_descent = 0.0_f32;
            let mut positioned: Vec<(f32, u32, f32, f32, f32, FontKey)> = Vec::new();

            for run in &line_glyphs {
                for g in &run.glyphs {
                    let rasterized = font_system.get_or_rasterize(
                        run.font_key,
                        g.glyph_id,
                        style.font_size,
                    );
                    let (w, h, bx, by) = match rasterized {
                        Some(r) => (r.width as f32, r.height as f32, r.bearing_x as f32, r.bearing_y as f32),
                        None => (0.0, 0.0, 0.0, 0.0),
                    };

                    let ascent = by;
                    let descent = (h - by).max(0.0);
                    max_ascent = max_ascent.max(ascent);
                    max_descent = max_descent.max(descent);

                    let x_pos = line_width + g.x_offset + bx;
                    let y_pos = ascent - by + g.y_offset;

                    positioned.push((x_pos, g.glyph_id, w, h, y_pos, run.font_key));
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
                .map(|(x, gid, w, h, y, fk)| LayoutGlyph {
                    glyph_id: gid,
                    x,
                    y: current_y + y,
                    width: w,
                    height: h,
                    color: style.color,
                    font_key: Some(fk),
                    font_size: style.font_size,
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

        TextLayout {
            lines: all_lines,
            width: max_line_width,
            height: current_y,
        }
    }

    /// Produces a fully laid-out `TextLayout` for the given text.
    ///
    /// Internally runs bidi resolution, line breaking, shaping, and
    /// rasterization. Glyph bitmaps are cached across calls to avoid
    /// re-rasterizing identical characters at the same size.
    ///
    /// For more control over line breaking, use [`Self::shape_text`] +
    /// [`Self::layout_lines`] instead.
    ///
    /// Available only with the **`layout-text`** feature (enabled by default).
    #[cfg(feature = "layout-text")]
    pub fn layout_text(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        style: &TextStyle<'_>,
        max_width: Option<f32>,
    ) -> TextLayout {
        let shaped = self.shape_text(font_system, text, style);
        let wrap_width = max_width.unwrap_or(f32::MAX);
        let line_ranges = build_line_ranges(text.len(), &shaped.fragments, wrap_width);
        self.layout_lines(font_system, &shaped, &line_ranges, style)
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
fn build_visual_runs(text: &str, base_direction: Direction, levels: &[u8]) -> Vec<VisualRun> {
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

/// Groups fragments into line ranges using greedy wrapping.
///
/// This is a convenience used by [`TextLayouter::layout_text`]; external
/// layout engines implement their own line-breaking logic.
#[cfg(feature = "layout-text")]
fn build_line_ranges(
    text_len: usize,
    fragments: &[Fragment],
    max_width: f32,
) -> Vec<(usize, usize)> {
    if text_len == 0 {
        return Vec::new();
    }
    if fragments.is_empty() {
        return vec![(0, text_len)];
    }

    if max_width >= f32::MAX || max_width <= 0.0 {
        return vec![(0, text_len)];
    }

    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut line_start: usize = 0;
    let mut last_break: Option<usize> = None;
    let mut line_width = 0.0_f32;

    for i in 0..fragments.len() {
        let frag = &fragments[i];
        line_width += frag.width;

        if line_width > max_width {
            let break_at = last_break.unwrap_or(frag.cluster);
            if break_at > line_start {
                lines.push((line_start, break_at));
            }
            line_start = break_at;
            line_width = 0.0;
            last_break = None;
            if break_at <= frag.cluster {
                line_width = frag.width;
            }
        }

        if frag.break_after {
            let next = fragments.get(i + 1).map(|f| f.cluster).unwrap_or(text_len);
            last_break = Some(next);
        }
    }

    if line_start < text_len {
        lines.push((line_start, text_len));
    }

    lines
}
