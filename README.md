# orinium_text

A CPU-side 2D text layout and software rendering library for Rust.

## Pipeline

```
Text → Bidi resolution → Visual runs → Shaping (rustybuzz) → Line breaking → Glyph rasterization (fontdue) → Positioned glyphs → Software renderer → RGBA image
```

## Dependencies

| Stage | Crate |
|-------|-------|
| Font discovery | `fontdb` |
| Shaping (glyph IDs, kerning, ligatures) | `rustybuzz` (HarfBuzz binding) |
| Rasterization | `fontdue` |
| Bidirectional text | `unicode-bidi` |
| Line breaking | `unicode-linebreak` |

## Concepts

- **Cluster**: A sequence of characters that map to one or more glyphs (e.g. a base character + combining marks). In the API, `Fragment::cluster` is the byte offset of the cluster in the original text.
- **Fragment**: The atomic layout unit — one glyph cluster with its advance width and line-break eligibility.
- **ShapedRun**: Internal per-font, per-direction run of positioned glyphs.
- **Visual run**: A contiguous span of text with the same resolved bidi direction.
- **RasterizedGlyph**: Public metrics struct (`width`, `height`, `bearing_x`, `bearing_y`) returned by the bitmap cache.
- **RgbaImage**: CPU-side RGBA pixel buffer, independent of any GPU API.

## Usage

```rust
use orinium_text::{FontSystem, TextLayouter, TextStyle, RgbaImage};

let mut font_system = FontSystem::new();
let mut layouter = TextLayouter::new();
let style = TextStyle::default();

// 1. Shape the text (bidi resolution, font shaping, break opportunities)
let shaped = layouter.shape_text(&mut font_system, "Hello, world!", &style);
// shaped.fragments — Vec<Fragment> with cluster byte offset, x, width, break_after

// 2. Determine line breaks (custom logic or external layout engine)
let line_ranges = vec![(0, 13)];  // (start_byte, end_byte) in the original text

// 3. Position glyphs into lines
let layout = layouter.layout_lines(&mut font_system, &shaped, &line_ranges, &style);

for line in &layout.lines {
    for glyph in &line.glyphs {
        // glyph.glyph_id, glyph.x, glyph.y, glyph.width, glyph.height,
        // glyph.font_key, glyph.font_size
    }
}

// 4. (optional) Render to an RGBA image
#[cfg(feature = "render")]
let image = orinium_text::render_text(&mut font_system, &layout, orinium_text::Color(255, 255, 255, 255));
// image.data — raw RGBA bytes, image.width, image.height
```

### Loading custom fonts

```rust
let font_data = std::fs::read("path/to/font.ttf").unwrap();
let mut font_system = FontSystem::new_with_fonts(vec![font_data]);
```

## Features

- `render` (default): Enables the software renderer (`render_text`) and [`RgbaImage`].
- `layout-text` (default): Enables the convenience `layout_text()` method and `build_line_ranges()`.
