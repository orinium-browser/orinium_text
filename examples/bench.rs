use orinium_text::{
    BidiMode, Color, FontStyle, FontSystem, FontVariant, FontWeight, TextLayouter, TextStyle,
};
use std::hint::black_box;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BenchCase {
    name: &'static str,
    text: &'static str,
    font_size: f32,
    weight: u16,
    style: FontStyle,
    variant: FontVariant,
    families: Vec<fontdb::Family<'static>>,
}

#[derive(Debug, Clone, Copy)]
struct BenchResult {
    shape: Duration,
    layout: Duration,
    rasterize: Duration,
    glyphs: usize,
    atlas_like_misses: usize,
    width: f32,
    height: f32,
}

fn main() {
    let cases = vec![
        BenchCase {
            name: "h1",
            text: "CAT IS GOD. CAT IS COD. AND CAT IS GOD. HAHAHAHAHAHAHA",
            font_size: 32.0,
            weight: 700,
            style: FontStyle::Normal,
            variant: FontVariant::Normal,
            families: sans_serif(),
        },
        BenchCase {
            name: "h1-italic",
            text: "CAT IS GOD. CAT IS COD. AND CAT IS GOD. HAHAHAHAHAHAHA",
            font_size: 32.0,
            weight: 700,
            style: FontStyle::Italic,
            variant: FontVariant::Normal,
            families: sans_serif(),
        },
        BenchCase {
            name: "body-fragment",
            text: "The world is destined to be encroached upon by cats, reshaped by cats, and devoured by cats.",
            font_size: 16.0,
            weight: 1600,
            style: FontStyle::Normal,
            variant: FontVariant::Normal,
            families: sans_serif(),
        },
        BenchCase {
            name: "inline-code",
            text: "inline code",
            font_size: 16.0,
            weight: 400,
            style: FontStyle::Normal,
            variant: FontVariant::Normal,
            families: monospace(),
        },
        BenchCase {
            name: "italic",
            text: "The world is destined to be encroached upon by cats, reshaped by cats, and devoured by cats.",
            font_size: 16.0,
            weight: 1600,
            style: FontStyle::Italic,
            variant: FontVariant::Normal,
            families: sans_serif(),
        },
        BenchCase {
            name: "subscript",
            text: "The world is destined to be encroached upon by cats, reshaped by cats, and devoured by cats.",
            font_size: 16.0,
            weight: 1600,
            style: FontStyle::Normal,
            variant: FontVariant::Subscript,
            families: sans_serif(),
        },
        BenchCase {
            name: "japanese",
            text: "あいうえおかきくけこabcdefghijklmnopqrstuvwxyz１２３４５６７８９０1234567890<>@;；＜＞",
            font_size: 16.0,
            weight: 1600,
            style: FontStyle::Normal,
            variant: FontVariant::Normal,
            families: sans_serif(),
        },
    ];

    println!("orinium_text bench");
    println!("------------------");
    println!();

    bench_cold_process_order(&cases);
    println!();
    bench_warm_repeated(&cases, 10);
}

fn bench_cold_process_order(cases: &[BenchCase]) {
    println!("== cold-ish pass ==");
    println!("note: global caches may warm after the first case");
    println!(
        "{:<18} {:>10} {:>10} {:>10} {:>7} {:>7} {:>10}",
        "case", "shape", "layout", "raster", "glyphs", "slow_calls", "size"
    );

    for case in cases {
        let mut fs = FontSystem::new();
        let mut layouter = TextLayouter::new();
        let result = run_case(&mut fs, &mut layouter, case, true);
        print_result(case.name, result);
    }
}

fn bench_warm_repeated(cases: &[BenchCase], iterations: usize) {
    println!("== warm repeated ==");
    println!("iterations={iterations}");
    println!(
        "{:<18} {:>10} {:>10} {:>10} {:>7} {:>7} {:>10}",
        "case", "shape", "layout", "raster", "glyphs", "misses", "size"
    );

    let mut fs = FontSystem::new();
    let mut layouter = TextLayouter::new();

    for case in cases {
        let _ = run_case(&mut fs, &mut layouter, case, true);

        let mut shape = Duration::ZERO;
        let mut layout = Duration::ZERO;
        let mut rasterize = Duration::ZERO;
        let mut last = None;

        for _ in 0..iterations {
            let result = run_case(&mut fs, &mut layouter, case, true);
            shape += result.shape;
            layout += result.layout;
            rasterize += result.rasterize;
            last = Some(result);
        }

        let Some(last) = last else {
            continue;
        };

        let averaged = BenchResult {
            shape: shape / iterations as u32,
            layout: layout / iterations as u32,
            rasterize: rasterize / iterations as u32,
            glyphs: last.glyphs,
            atlas_like_misses: last.atlas_like_misses,
            width: last.width,
            height: last.height,
        };

        print_result(case.name, averaged);
    }
}

fn run_case(
    fs: &mut FontSystem,
    layouter: &mut TextLayouter,
    case: &BenchCase,
    rasterize: bool,
) -> BenchResult {
    let style = TextStyle {
        font_size: case.font_size,
        color: Color(0, 0, 0, 255),
        font_weight: FontWeight(case.weight),
        font_style: case.style,
        line_height: 1.2,
        bidi_mode: BidiMode::Auto,
        font_families: case.families.clone(),
        exact_fonts: Vec::new(),
        variant: case.variant,
    };

    let shape_start = Instant::now();
    let shaped = layouter.shape_text(fs, case.text, &style);
    let shape = shape_start.elapsed();

    let line_ranges = line_ranges_for(case.text);

    let layout_start = Instant::now();
    let layout = layouter.layout_lines(fs, &shaped, &line_ranges, &style);
    let layout_elapsed = layout_start.elapsed();

    let mut rasterize_elapsed = Duration::ZERO;
    let mut glyphs = 0usize;
    let mut misses = 0usize;

    if rasterize {
        let rasterize_start = Instant::now();

        for line in &layout.lines {
            for glyph in &line.glyphs {
                glyphs += 1;

                let Some(font_key) = glyph.font_key else {
                    continue;
                };

                let before = Instant::now();
                let bitmap =
                    fs.get_or_rasterize_with_bitmap(font_key, glyph.glyph_id, glyph.font_size);
                let elapsed = before.elapsed();

                if elapsed > Duration::from_micros(100) {
                    misses += 1;
                }

                black_box(bitmap);
            }
        }

        rasterize_elapsed = rasterize_start.elapsed();
    } else {
        for line in &layout.lines {
            glyphs += line.glyphs.len();
        }
    }

    black_box(&layout);

    BenchResult {
        shape,
        layout: layout_elapsed,
        rasterize: rasterize_elapsed,
        glyphs,
        atlas_like_misses: misses,
        width: layout.width,
        height: layout.height,
    }
}

fn line_ranges_for(text: &str) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = 0usize;

    for part in text.split_inclusive('\n') {
        let end = start + part.trim_end_matches('\n').len();
        ranges.push((start, end));
        start += part.len();
    }

    if ranges.is_empty() {
        ranges.push((0, text.len()));
    }

    ranges
}

fn print_result(name: &str, result: BenchResult) {
    println!(
        "{:<18} {:>10} {:>10} {:>10} {:>7} {:>7} {:>5.1}x{:<4.1}",
        name,
        fmt_duration(result.shape),
        fmt_duration(result.layout),
        fmt_duration(result.rasterize),
        result.glyphs,
        result.atlas_like_misses,
        result.width,
        result.height,
    );
}

fn fmt_duration(duration: Duration) -> String {
    if duration.as_secs_f64() >= 1.0 {
        format!("{:.3}s", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{:.3}ms", duration.as_secs_f64() * 1000.0)
    } else {
        format!("{:.3}µs", duration.as_secs_f64() * 1_000_000.0)
    }
}

fn sans_serif() -> Vec<fontdb::Family<'static>> {
    vec![
        fontdb::Family::Name("DejaVu Sans"),
        fontdb::Family::Name("Liberation Sans"),
        fontdb::Family::SansSerif,
        fontdb::Family::Serif,
        fontdb::Family::Monospace,
    ]
}

fn monospace() -> Vec<fontdb::Family<'static>> {
    vec![
        fontdb::Family::Name("DejaVu Sans Mono"),
        fontdb::Family::Name("Liberation Mono"),
        fontdb::Family::Monospace,
        fontdb::Family::SansSerif,
        fontdb::Family::Serif,
    ]
}
