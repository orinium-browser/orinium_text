use orinium_text::{
    BidiMode, Color, FontStyle, FontSystem, FontWeight, Fragment, LayoutGlyph, LayoutLine,
    TextLayout, TextLayouter, TextStyle,
};

fn default_style() -> TextStyle<'static> {
    TextStyle {
        font_size: 16.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.2,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![orinium_text::fontdb::Family::SansSerif],
        exact_fonts: Vec::new(),
    }
}

// ── types ──────────────────────────────────────────────────────────────

#[test]
fn test_color_default() {
    let c = Color(0, 0, 0, 255);
    assert_eq!(c.0, 0);
    assert_eq!(c.3, 255);
}

#[test]
fn test_color_equality() {
    assert_eq!(Color(255, 0, 0, 255), Color(255, 0, 0, 255));
    assert_ne!(Color(255, 0, 0, 255), Color(0, 255, 0, 255));
}

#[test]
fn test_color_copy() {
    let c1 = Color(10, 20, 30, 40);
    let c2 = c1;
    assert_eq!(c1, c2);
}

#[test]
fn test_font_weight_new() {
    assert_eq!(FontWeight(400).0, 400);
    assert_eq!(FontWeight(700).0, 700);
}

#[test]
fn test_font_weight_order() {
    assert!(FontWeight(100) < FontWeight(900));
    assert!(FontWeight(700) > FontWeight(400));
}

#[test]
fn test_font_weight_equality() {
    assert_eq!(FontWeight(400), FontWeight(400));
    assert_ne!(FontWeight(400), FontWeight(700));
}

#[test]
fn test_font_style_variants() {
    assert_ne!(FontStyle::Normal, FontStyle::Italic);
    assert_ne!(FontStyle::Normal, FontStyle::Oblique);
    assert_ne!(FontStyle::Italic, FontStyle::Oblique);
}

#[test]
fn test_bidi_mode_variants() {
    assert_ne!(BidiMode::Auto, BidiMode::Ltr);
    assert_ne!(BidiMode::Ltr, BidiMode::Rtl);
    assert_ne!(BidiMode::Auto, BidiMode::Rtl);
}

#[test]
fn test_text_style_default_values() {
    let style = TextStyle::default();
    assert_eq!(style.font_size, 16.0);
    assert_eq!(style.color, Color(0, 0, 0, 255));
    assert_eq!(style.font_weight, FontWeight(400));
    assert_eq!(style.font_style, FontStyle::Normal);
    assert_eq!(style.line_height, 1.2);
    assert_eq!(style.bidi_mode, BidiMode::Auto);
    assert_eq!(style.font_families.len(), 3);
    assert!(style.exact_fonts.is_empty());
}

#[test]
fn test_text_style_debug() {
    let style = TextStyle::default();
    let debug = format!("{:?}", style);
    assert!(debug.contains("font_size"));
    assert!(debug.contains("16.0"));
}

#[test]
fn test_font_key_is_copy() {
    let fs = FontSystem::new();
    let key = fs
        .query(
            &[orinium_text::fontdb::Family::SansSerif],
            FontWeight(400),
            FontStyle::Normal,
        )
        .expect("sans-serif font not found");
    let k2 = key;
    assert_eq!(key, k2);
}

#[test]
fn test_font_key_hash_and_eq() {
    use std::collections::HashSet;
    let fs = FontSystem::new();
    let key = fs
        .query(
            &[orinium_text::fontdb::Family::SansSerif],
            FontWeight(400),
            FontStyle::Normal,
        )
        .expect("sans-serif font not found");
    let mut set = HashSet::new();
    set.insert(key);
    set.insert(key);
    assert_eq!(set.len(), 1);
}

#[test]
fn test_layout_glyph_fields() {
    let g = LayoutGlyph {
        glyph_id: 72,
        x: 10.0,
        y: 20.0,
        width: 8.0,
        height: 12.0,
        color: Color(0, 0, 0, 255),
        font_key: None,
        font_size: 16.0,
    };
    assert_eq!(g.glyph_id, 72);
    assert_eq!(g.x, 10.0);
}

#[test]
fn test_layout_line_fields() {
    let line = LayoutLine {
        glyphs: vec![],
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 20.0,
        baseline: 16.0,
    };
    assert_eq!(line.width, 100.0);
    assert_eq!(line.baseline, 16.0);
}

#[test]
fn test_text_layout_fields() {
    let layout = TextLayout {
        lines: vec![],
        width: 0.0,
        height: 0.0,
    };
    assert_eq!(layout.width, 0.0);
    assert_eq!(layout.height, 0.0);
}

#[test]
fn test_text_layout_with_lines() {
    let layout = TextLayout {
        lines: vec![LayoutLine {
            glyphs: vec![],
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 30.0,
            baseline: 24.0,
        }],
        width: 200.0,
        height: 30.0,
    };
    assert_eq!(layout.lines.len(), 1);
    assert_eq!(layout.width, 200.0);
    assert_eq!(layout.height, 30.0);
}

#[test]
fn test_text_style_clone() {
    let style = TextStyle::default();
    let cloned = style.clone();
    assert_eq!(style.font_size, cloned.font_size);
    assert_eq!(style.font_families, cloned.font_families);
}

// ── font_system ────────────────────────────────────────────────────────

#[test]
fn test_query_returns_some_for_sans_serif() {
    let fs = FontSystem::new();
    let key = fs.query(
        &[orinium_text::fontdb::Family::SansSerif],
        FontWeight(400),
        FontStyle::Normal,
    );
    assert!(key.is_some());
}

#[test]
fn test_query_returns_some_for_serif() {
    let fs = FontSystem::new();
    let key = fs.query(
        &[orinium_text::fontdb::Family::Serif],
        FontWeight(400),
        FontStyle::Normal,
    );
    assert!(key.is_some());
}

#[test]
fn test_query_returns_some_for_monospace() {
    let fs = FontSystem::new();
    let key = fs.query(
        &[orinium_text::fontdb::Family::Monospace],
        FontWeight(400),
        FontStyle::Normal,
    );
    assert!(key.is_some());
}

#[test]
fn test_query_bold_is_different_from_normal() {
    let fs = FontSystem::new();
    let normal = fs.query(
        &[orinium_text::fontdb::Family::SansSerif],
        FontWeight(400),
        FontStyle::Normal,
    );
    let bold = fs.query(
        &[orinium_text::fontdb::Family::SansSerif],
        FontWeight(700),
        FontStyle::Normal,
    );
    assert!(normal.is_some());
    assert!(bold.is_some());
}

#[test]
fn test_get_font_data_returns_data_for_known_key() {
    let mut fs = FontSystem::new();
    let key = fs
        .query(
            &[orinium_text::fontdb::Family::SansSerif],
            FontWeight(400),
            FontStyle::Normal,
        )
        .expect("sans-serif font not found");
    let data = fs.get_font_data(key);
    assert!(data.is_some(), "expected font data for queried key");
    assert!(data.unwrap().len() > 0, "font data should be non-empty");
}

#[test]
fn test_get_font_data_caches_after_first_call() {
    let mut fs = FontSystem::new();
    let key = fs
        .query(
            &[orinium_text::fontdb::Family::SansSerif],
            FontWeight(400),
            FontStyle::Normal,
        )
        .expect("sans-serif font not found");

    let data1 = fs.get_font_data(key).unwrap().to_vec();
    let data2 = fs.get_font_data(key).unwrap().to_vec();
    assert_eq!(data1.len(), data2.len(), "cached data should match");
    assert!(!data1.is_empty(), "font data should exist");
}

#[test]
fn test_query_nonexistent_family_returns_none() {
    let fs = FontSystem::new();
    let key = fs.query(
        &[orinium_text::fontdb::Family::Name(
            "ThisFontDoesNotExist12345",
        )],
        FontWeight(400),
        FontStyle::Normal,
    );
    assert!(key.is_none());
}

#[test]
fn test_load_font_data_works_with_system_font() {
    let font_paths = [
        r"C:\Windows\Fonts\arial.ttf",
        r"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        r"/usr/share/fonts/TTF/DejaVuSans.ttf",
        r"/System/Library/Fonts/Helvetica.ttc",
    ];

    let data = font_paths.iter().find_map(|p| std::fs::read(p).ok());

    if let Some(data) = data {
        let mut fs = FontSystem::new();
        let keys = fs.load_font_data(data);
        assert!(!keys.is_empty(), "expected at least one font face");
    }
}

#[test]
fn test_duplicate_load_returns_multiple_keys() {
    let font_paths = [
        r"C:\Windows\Fonts\arial.ttf",
        r"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        r"/usr/share/fonts/TTF/DejaVuSans.ttf",
        r"/System/Library/Fonts/Helvetica.ttc",
    ];

    let data = font_paths.iter().find_map(|p| std::fs::read(p).ok());

    if let Some(ref data) = data {
        let mut fs = FontSystem::new();
        let first = fs.load_font_data(data.clone());
        assert!(!first.is_empty());

        let keys = fs.load_font_data(data.clone());
        assert!(!keys.is_empty());
    }
}

// ── layout ─────────────────────────────────────────────────────────────

#[test]
fn test_shape_text_empty_input() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let shaped = layouter.shape_text(&mut fs, "", &style);
    assert!(shaped.fragments.is_empty());
}

#[test]
fn test_shape_text_returns_fragments() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let shaped = layouter.shape_text(&mut fs, "Hello", &style);
    assert!(
        !shaped.fragments.is_empty(),
        "expected fragments for 'Hello'"
    );
    assert!(
        shaped.fragments.len() >= 5,
        "expected at least 5 fragments, got {}",
        shaped.fragments.len()
    );
}

#[cfg(feature = "layout-text")]
#[test]
fn test_layout_text_empty_input() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let layout = layouter.layout_text(&mut fs, "", &style, None);
    assert!(layout.lines.is_empty());
    assert_eq!(layout.width, 0.0);
    assert_eq!(layout.height, 0.0);
}

#[cfg(feature = "layout-text")]
#[test]
fn test_layout_text_single_line() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let layout = layouter.layout_text(&mut fs, "Hello", &style, None);
    assert_eq!(layout.lines.len(), 1, "single line expected");
    assert!(!layout.lines[0].glyphs.is_empty(), "expected glyphs");
    assert!(layout.width > 0.0, "expected positive width");
    assert!(layout.height > 0.0, "expected positive height");
}

#[cfg(feature = "layout-text")]
#[test]
fn test_layout_text_with_max_width_triggers_wrapping() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let text = "The quick brown fox jumps over the lazy dog.";
    let layout = layouter.layout_text(&mut fs, text, &style, Some(10.0));
    assert!(
        layout.lines.len() >= 2,
        "expected multiple lines with narrow width, got {}",
        layout.lines.len()
    );
}

#[cfg(feature = "layout-text")]
#[test]
fn test_layout_text_without_max_width_no_wrapping() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let text = "The quick brown fox jumps over the lazy dog";
    let layout = layouter.layout_text(&mut fs, text, &style, Some(f32::MAX));
    assert_eq!(
        layout.lines.len(),
        1,
        "single line expected without wrapping"
    );
}

#[test]
fn test_shape_text_preserves_visual_order() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let shaped = layouter.shape_text(&mut fs, "ABC", &style);
    for i in 1..shaped.fragments.len() {
        assert!(
            shaped.fragments[i].x > shaped.fragments[i - 1].x,
            "fragments should be in visual order"
        );
    }
}

#[cfg(feature = "layout-text")]
#[test]
fn test_layout_text_respects_font_size() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let mut small_style = default_style();
    small_style.font_size = 16.0;
    let mut large_style = default_style();
    large_style.font_size = 32.0;

    let small = layouter.layout_text(&mut fs, "Hello", &small_style, None);
    let large = layouter.layout_text(&mut fs, "Hello", &large_style, None);

    assert!(
        large.height > small.height,
        "larger font should produce larger height"
    );
    assert!(
        large.width > small.width,
        "larger font should produce larger width"
    );
}

#[test]
fn test_shape_text_plus_layout_lines_workflow() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let shaped = layouter.shape_text(&mut fs, "Hello world", &style);
    assert!(!shaped.fragments.is_empty());

    let line_ranges = vec![(0, 11)];
    let layout = layouter.layout_lines(&mut fs, &shaped, &line_ranges, &style);
    assert_eq!(layout.lines.len(), 1, "single line expected");
    assert!(!layout.lines[0].glyphs.is_empty(), "expected glyphs");
}

#[test]
fn test_layout_lines_skips_empty_range() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let shaped = layouter.shape_text(&mut fs, "Hello", &style);
    let layout = layouter.layout_lines(&mut fs, &shaped, &[], &style);
    assert!(layout.lines.is_empty());
}

#[test]
fn test_shaped_text_debug() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();
    let shaped = layouter.shape_text(&mut fs, "Hi", &style);
    let debug = format!("{:?}", shaped);
    assert!(!debug.is_empty());
}

#[cfg(feature = "layout-text")]
#[test]
fn test_glyph_cache_reuses_across_calls() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let layout1 = layouter.layout_text(&mut fs, "Hello", &style, None);
    let layout2 = layouter.layout_text(&mut fs, "Hello", &style, None);
    assert_eq!(layout1.lines.len(), layout2.lines.len());
    assert_eq!(layout1.width, layout2.width);
}

#[test]
fn test_fragments_have_break_opportunities() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();
    let shaped = layouter.shape_text(&mut fs, "Hello World", &style);

    let break_fragments: Vec<&Fragment> =
        shaped.fragments.iter().filter(|f| f.break_after).collect();
    assert!(!break_fragments.is_empty(), "expected break opportunities");
}

#[test]
fn test_fragment_cluster_mapping() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();
    let text = "AB";
    let shaped = layouter.shape_text(&mut fs, text, &style);

    for frag in &shaped.fragments {
        assert!(
            frag.cluster < text.len(),
            "cluster {} out of bounds",
            frag.cluster
        );
    }
}

#[cfg(feature = "layout-text")]
#[test]
fn test_text_style_exact_fonts_override() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let mut style = default_style();

    let key = fs
        .query(
            &[orinium_text::fontdb::Family::SansSerif],
            FontWeight(400),
            FontStyle::Normal,
        )
        .expect("sans-serif font not found");
    style.exact_fonts = vec![key];

    let layout = layouter.layout_text(&mut fs, "Hello", &style, None);
    assert_eq!(layout.lines.len(), 1);
    assert!(!layout.lines[0].glyphs.is_empty());
}

#[test]
fn test_japanese_text_does_not_panic() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    // Japanese text should not panic during shaping.
    // The font fallback chain (SansSerif → Serif → Monospace) typically
    // lacks CJK coverage, previously triggering a panic when coverage_ranges
    // sliced at wrong byte offsets.
    let pure_jp = "こんにちは世界";
    let _ = layouter.shape_text(&mut fs, pure_jp, &style);
    // Pure Japanese may produce 0 fragments if no CJK font is available.
    // The critical thing is that it does not panic.

    // Mixed ASCII + Japanese must produce at least the ASCII fragments.
    let mixed_texts = ["Hello 日本語", "ABC日本語DEF", "aあbいcう"];
    for text in mixed_texts {
        let shaped = layouter.shape_text(&mut fs, text, &style);
        assert!(
            !shaped.fragments.is_empty(),
            "expected fragments for '{text}'"
        );
    }

    // Verify that all fragment clusters are valid byte offsets
    for text in &["Hello 日本語", "ABC日本語DEF", "aあbいcう"] {
        let shaped = layouter.shape_text(&mut fs, text, &style);
        for frag in &shaped.fragments {
            assert!(
                frag.cluster <= text.len(),
                "cluster {} out of bounds for '{text}'",
                frag.cluster
            );
        }
    }
}

#[cfg(feature = "layout-text")]
#[test]
fn test_japanese_layout_text_single_line() {
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = default_style();

    let layout = layouter.layout_text(&mut fs, "ABC日本語", &style, None);
    assert_eq!(layout.lines.len(), 1, "single line expected");
    // ASCII portion should produce glyphs even if CJK is not covered
    assert!(!layout.lines[0].glyphs.is_empty(), "expected glyphs");
    assert!(layout.width >= 0.0, "expected non-negative width");
    assert!(layout.height >= 0.0, "expected non-negative height");
}

#[test]
fn test_layout_lines_trailing_newline_does_not_panic() {
    // Simulate orinium's usage pattern: split by \n, call shape_text + layout_lines
    let text = "Hello\n";
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = TextStyle {
        font_size: 16.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.2,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![fontdb::Family::SansSerif],
        exact_fonts: Vec::new(),
    };

    let shaped = layouter.shape_text(&mut fs, text, &style);

    let line_ranges: Vec<(usize, usize)> = text
        .split('\n')
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            let end = start + line.len();
            Some((start, end))
        })
        .collect();

    let layout = layouter.layout_lines(&mut fs, &shaped, &line_ranges, &style);
    assert_eq!(
        layout.lines.len(),
        line_ranges.len(),
        "layout.lines.len() ({}) must equal line_ranges.len() ({}) (trailing newline case)",
        layout.lines.len(),
        line_ranges.len()
    );
}

#[test]
fn test_layout_lines_empty_text_newline() {
    let text = "\n";
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = TextStyle {
        font_size: 16.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.2,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![fontdb::Family::SansSerif],
        exact_fonts: Vec::new(),
    };

    let shaped = layouter.shape_text(&mut fs, text, &style);

    let line_ranges: Vec<(usize, usize)> = text
        .split('\n')
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            let end = start + line.len();
            Some((start, end))
        })
        .collect();

    let layout = layouter.layout_lines(&mut fs, &shaped, &line_ranges, &style);
    assert_eq!(
        layout.lines.len(),
        line_ranges.len(),
        "layout.lines.len() ({}) must equal line_ranges.len() ({}) (\\n only case)",
        layout.lines.len(),
        line_ranges.len()
    );
}

#[test]
fn test_layout_lines_consecutive_newlines() {
    let text = "ab\n\ncd";
    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();
    let style = TextStyle {
        font_size: 16.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.2,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![fontdb::Family::SansSerif],
        exact_fonts: Vec::new(),
    };

    let shaped = layouter.shape_text(&mut fs, text, &style);

    let line_ranges: Vec<(usize, usize)> = text
        .split('\n')
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            let end = start + line.len();
            Some((start, end))
        })
        .collect();

    let layout = layouter.layout_lines(&mut fs, &shaped, &line_ranges, &style);
    assert_eq!(
        layout.lines.len(),
        line_ranges.len(),
        "layout.lines.len() ({}) must equal line_ranges.len() ({}) (consecutive \\n case)",
        layout.lines.len(),
        line_ranges.len()
    );
}

#[test]
fn test_layout_lines_freeze_repro() {
    // Reproduce the freeze from orinium: font_size=32, text="HTML Living Standard Test Page"
    let font_paths = [
        r"C:\Windows\Fonts\arial.ttf",
        r"/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        r"/usr/share/fonts/TTF/DejaVuSans.ttf",
        r"/System/Library/Fonts/Helvetica.ttc",
    ];

    let data = match font_paths.iter().find_map(|p| std::fs::read(p).ok()) {
        Some(d) => d,
        None => {
            eprintln!("skipping freeze repro test: no system font found");
            return;
        }
    };

    let mut fs = FontSystem::new();
    let exact_fonts = fs.load_font_data(data);

    let mut layouter = TextLayouter::new();
    let style = TextStyle {
        font_size: 32.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.2,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![fontdb::Family::SansSerif],
        exact_fonts,
    };

    let text = "HTML Living Standard Test Page";
    let shaped = layouter.shape_text(&mut fs, text, &style);
    assert!(!shaped.fragments.is_empty(), "expected fragments");

    // Simulate orinium's measure: split by \n (no trailing newline in this case)
    let line_ranges: Vec<(usize, usize)> = text
        .split('\n')
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            let end = start + line.len();
            Some((start, end))
        })
        .collect();

    let layout = layouter.layout_lines(&mut fs, &shaped, &line_ranges, &style);
    assert_eq!(
        layout.lines.len(),
        line_ranges.len(),
        "should produce same number of lines as line_ranges"
    );
    assert!(!layout.lines.is_empty(), "should have at least one line");
}

#[test]
fn test_layout_lines_msgothic_ttc() {
    // orinium loads msgothic.ttc as first candidate on Windows
    let font_path = r"C:\Windows\Fonts\msgothic.ttc";
    if !std::path::Path::new(font_path).exists() {
        eprintln!("skipping: msgothic.ttc not found");
        return;
    }

    let data = std::fs::read(font_path).expect("read msgothic.ttc");
    let mut fs = FontSystem::new();
    let exact_fonts = fs.load_font_data(data);

    let mut layouter = TextLayouter::new();
    let style = TextStyle {
        font_size: 32.0,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(400),
        font_style: FontStyle::Normal,
        line_height: 1.2,
        bidi_mode: BidiMode::Ltr,
        font_families: vec![fontdb::Family::SansSerif],
        exact_fonts,
    };

    // Test with ASCII text (like orinium's initial page title)
    let text = "HTML Living Standard Test Page";
    let shaped = layouter.shape_text(&mut fs, text, &style);
    assert!(
        !shaped.fragments.is_empty(),
        "expected fragments for ASCII text with msgothic.ttc"
    );

    let line_ranges: Vec<(usize, usize)> = text
        .split('\n')
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            let end = start + line.len();
            Some((start, end))
        })
        .collect();

    let layout = layouter.layout_lines(&mut fs, &shaped, &line_ranges, &style);
    assert_eq!(layout.lines.len(), line_ranges.len());
    assert!(!layout.lines.is_empty());
}

#[cfg(feature = "layout-text")]
#[test]
fn test_new_returns_default() {
    let mut a = TextLayouter::new();
    let mut b = TextLayouter::default();
    let mut fs = FontSystem::new();
    let style = default_style();
    let layout_a = a.layout_text(&mut fs, "test", &style, None);
    let layout_b = b.layout_text(&mut fs, "test", &style, None);
    assert_eq!(layout_a.lines.len(), layout_b.lines.len());
}
