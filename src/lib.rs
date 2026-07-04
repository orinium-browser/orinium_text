//! A minimal text layout library.
//!
//! Provides font loading, text shaping, glyph rasterization, line layout,
//! and software rendering — all CPU-side with no GPU dependencies.

pub mod font_system;
pub mod image;
pub mod layout;
pub mod types;

#[cfg(feature = "render")]
pub mod render;

pub use font_system::{FontSystem, PlatformFallback};
pub use image::RgbaImage;
pub use layout::{Fragment, RasterizedGlyph, ShapedText, TextLayouter};
pub use types::*;

#[cfg(feature = "render")]
pub use render::render_text;

// Re-exported so integration tests can construct TextStyle fields.
pub use fontdb;
