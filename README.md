# orinium_text

A 2D text layout library for Rust.

## Pipeline

```
Text → Bidi resolution → Visual runs → Shaping (rustybuzz) → Line breaking → Glyph rasterization (fontdue) → Positioned glyphs
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

## Usage

```rust
use orinium_text::{FontSystem, TextLayouter, TextStyle};

let mut font_system = FontSystem::new();
let mut layouter = TextLayouter::new();
let style = TextStyle::default();

// 1. Shape the text (bidi resolution, font shaping, break opportunities)
let shaped = layouter.shape_text(&mut font_system, "Hello, world!", &style);
// shaped.fragments — Vec<Fragment> with cluster byte offset, x, width, break_after

// 2. Determine line breaks (custom logic or external layout engine)
let line_ranges = vec![(0, 13)];  // (start_byte, end_byte) in the original text

// 3. Position glyphs into lines
let layout = layouter.layout_lines(&shaped, &line_ranges, &style);

for line in &layout.lines {
    for glyph in &line.glyphs {
        // glyph.glyph_id, glyph.x, glyph.y, glyph.width, glyph.height
    }
}
```

### Loading custom fonts

```rust
let font_data = std::fs::read("path/to/font.ttf").unwrap();
let mut font_system = FontSystem::new_with_fonts(vec![font_data]);
```

## Features

- `layout-text` (default): Enables the convenience `layout_text()` method and `build_line_ranges()`.
