use crate::font_system::FontSystem;
use crate::image::RgbaImage;
use crate::types::{Color, TextLayout};

/// Renders a laid-out `TextLayout` into an RGBA image.
///
/// Each glyph is rasterized on demand using the cached alpha masks in
/// `font_system` and composited onto a canvas sized to the layout bounds.
/// Glyph positions are rounded to the nearest pixel.
pub fn render_text(
    font_system: &mut FontSystem,
    layout: &TextLayout,
    background: Color,
) -> RgbaImage {
    let w = layout.width.ceil() as u32;
    let h = layout.height.ceil() as u32;
    if w == 0 || h == 0 {
        return RgbaImage::new(1, 1, background);
    }

    let mut image = RgbaImage::new(w, h, background);

    for line in &layout.lines {
        for glyph in &line.glyphs {
            let Some(font_key) = glyph.font_key else {
                continue;
            };
            let Some((metrics, alpha_mask)) =
                font_system.get_or_rasterize_with_bitmap(font_key, glyph.glyph_id, glyph.font_size)
            else {
                continue;
            };
            image.blend_alpha_mask(
                glyph.x,
                glyph.y,
                &alpha_mask,
                metrics.width,
                metrics.height,
                glyph.color,
            );
        }
    }

    image
}
