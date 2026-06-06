//! A minimal text layout library.
//!
//! Provides font loading, text shaping, glyph rasterization, and line layout
//! powered by [rustybuzz], [fontdue], and [fontdb].

pub mod font_system;
pub mod types;

pub use font_system::FontSystem;
pub use types::*;
