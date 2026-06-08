use crate::types::Color;

/// A CPU-side RGBA pixel buffer, independent of any GPU API.
///
/// Can be uploaded to a wgpu texture, written to a PNG, or converted to
/// `ImageData` for Web canvas — all from the caller's side.
#[derive(Debug, Clone)]
pub struct RgbaImage {
    /// RGBA pixel data, row-major, length = `width * height * 4`.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl RgbaImage {
    /// Creates a new image filled with the given `color`.
    pub fn new(width: u32, height: u32, color: Color) -> Self {
        let pixel = [color.0, color.1, color.2, color.3];
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            data.extend_from_slice(&pixel);
        }
        RgbaImage {
            data,
            width,
            height,
        }
    }

    /// Returns the raw RGBA byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Returns the raw RGBA byte slice, mutable.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Gets the RGBA color at the given pixel coordinate.
    ///
    /// Returns `Color(0, 0, 0, 0)` if the coordinate is out of bounds.
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.width || y >= self.height {
            return Color(0, 0, 0, 0);
        }
        let idx = ((y * self.width + x) * 4) as usize;
        Color(
            self.data[idx],
            self.data[idx + 1],
            self.data[idx + 2],
            self.data[idx + 3],
        )
    }

    /// Sets the pixel at the given coordinate to `color`.
    ///
    /// Does nothing if the coordinate is out of bounds.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.data[idx] = color.0;
        self.data[idx + 1] = color.1;
        self.data[idx + 2] = color.2;
        self.data[idx + 3] = color.3;
    }

    /// Blends an alpha mask onto the image at the given position.
    ///
    /// `alpha_mask` is a row-major `u8` array (`width × height` values).
    /// Each byte is used as the alpha channel for the glyph's `color`,
    /// composited over the existing pixel via standard over-blending.
    ///
    /// Coordinates are rounded to the nearest integer (sub-pixel positions
    /// are truncated to pixel boundaries).
    pub fn blend_alpha_mask(
        &mut self,
        x: f32,
        y: f32,
        alpha_mask: &[u8],
        mask_width: u32,
        mask_height: u32,
        color: Color,
    ) {
        let ox = x.round() as i32;
        let oy = y.round() as i32;

        let src_r = color.0 as u32;
        let src_g = color.1 as u32;
        let src_b = color.2 as u32;

        for my in 0..mask_height {
            let py = oy + my as i32;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for mx in 0..mask_width {
                let px = ox + mx as i32;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let mask_idx = (my * mask_width + mx) as usize;
                let a = alpha_mask[mask_idx] as u32;
                if a == 0 {
                    continue;
                }

                let dst_idx = ((py as u32 * self.width + px as u32) * 4) as usize;
                let dst_r = self.data[dst_idx] as u32;
                let dst_g = self.data[dst_idx + 1] as u32;
                let dst_b = self.data[dst_idx + 2] as u32;
                let dst_a = self.data[dst_idx + 3] as u32;

                // sRGB over: out = src * a/255 + dst * (1 - a/255)
                // Pre-multiplied weights to avoid per-channel division.
                let out_a = dst_a + a - (dst_a * a) / 255;
                if out_a == 0 {
                    continue;
                }
                let out_r = (src_r * a + dst_r * (255 - a)) / 255;
                let out_g = (src_g * a + dst_g * (255 - a)) / 255;
                let out_b = (src_b * a + dst_b * (255 - a)) / 255;

                self.data[dst_idx] = out_r as u8;
                self.data[dst_idx + 1] = out_g as u8;
                self.data[dst_idx + 2] = out_b as u8;
                self.data[dst_idx + 3] = out_a as u8;
            }
        }
    }
}
