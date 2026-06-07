//! Basic usage example for orinium_text.

use orinium_text::{BidiMode, Color, FontStyle, FontSystem, FontWeight, TextLayouter, TextStyle};

fn main() {
    let mut font_system = FontSystem::new();
    let mut layouter = TextLayouter::new();

    let style = TextStyle {
        font_size: 24.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.4,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![fontdb::Family::SansSerif],
        exact_fonts: Vec::new(),
    };

    // --- Two-step: shape → external line-breaking → layout ---
    let text2 = "The quick brown fox jumps over the lazy dog.";
    let shaped = layouter.shape_text(&mut font_system, text2, &style);
    println!("\n--- shape_text + layout_lines ---");
    println!("fragments: {} total", shaped.fragments.len());
    for (i, f) in shaped.fragments.iter().enumerate().take(20) {
        println!(
            "  fragment {i:>3}: cluster={:>3}  x={:>8.2}  width={:>8.2}  break={}",
            f.cluster, f.x, f.width, f.break_after
        );
    }
    if shaped.fragments.len() > 20 {
        println!("  ... {} more", shaped.fragments.len() - 20);
    }

    // Pass line ranges directly to layout_lines
    let line_ranges = vec![(0, text2.len())];
    let layout2 = layouter.layout_lines(&shaped, &line_ranges, &style);
    println!(
        "\n  layout_lines result: {} line(s), width={:.1}, height={:.1}",
        layout2.lines.len(),
        layout2.width,
        layout2.height
    );

    // --- Multi-line wrapping (only with layout-text feature) ---
    #[cfg(feature = "layout-text")]
    {
        let text3 = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
        let layout3 = layouter.layout_text(&mut font_system, text3, &style, Some(250.0));
        println!("\n--- Multi-line wrapping (max_width=250 px) ---");
        println!(
            "lines: {}  width: {:.1}  height: {:.1}",
            layout3.lines.len(),
            layout3.width,
            layout3.height
        );
        for (i, line) in layout3.lines.iter().enumerate() {
            println!(
                "  line {i}: {:.1}×{:.1} — {} glyph(s)",
                line.width,
                line.height,
                line.glyphs.len()
            );
        }
    }
}
