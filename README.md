# orinium_text

A text layout library for Orinium.

- **Shaping**: rustybuzz (HarfBuzz binding)
- **Rasterization**: fontdue
- **Font management**: fontdb
- **Bidi + line breaking**: unicode-bidi, unicode-linebreak

```rust
let mut font_system = FontSystem::new();
let mut layouter = TextLayouter::new();

let layout = layouter.layout_text(
    &mut font_system,
    "Hello, world!",
    &TextStyle::default(),
    Some(800.0),
);

for line in &layout.lines {
    for glyph in &line.glyphs {
        // glyph.x, glyph.y, glyph.width, glyph.height, glyph.glyph_id
    }
}
```
