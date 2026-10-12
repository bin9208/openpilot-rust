// SPDX-License-Identifier: LGPL-2.0-or-later
// Pygame 2.6.1 draw.c/alphablit.c algorithms translated to safe Rust.
// Copyright (C) 2000-2001 Pete Shinners; see provenance.json.
use super::{Error, Rect, Result};
use openpilot_sdl2::gui::Image;
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub rgba: Vec<u8>,
}
impl Canvas {
    /// # Errors
    /// Returns invalid dimensions or allocation failure.
    pub fn new(width: i32, height: i32) -> Result<Self> {
        let count = usize::try_from(width)
            .ok()
            .filter(|v| *v > 0)
            .and_then(|w| {
                usize::try_from(height)
                    .ok()
                    .filter(|v| *v > 0)
                    .and_then(|h| w.checked_mul(h))
            })
            .and_then(|s| s.checked_mul(4))
            .ok_or(Error::Contract("canvas dimensions invalid"))?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(count)
            .map_err(|_| Error::Contract("canvas allocation failed"))?;
        rgba.resize(count, 0);
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
    pub fn fill(&mut self, color: [u8; 3]) {
        for p in self.rgba.chunks_exact_mut(4) {
            p.copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        if let (Ok(x), Ok(y), Ok(width)) = (
            usize::try_from(x),
            usize::try_from(y),
            usize::try_from(self.width),
        ) {
            let index = (y * width + x) * 4;
            self.rgba[index..index + 3].copy_from_slice(&color);
        }
    }
    fn horizontal(&mut self, x1: i32, x2: i32, y: i32, color: [u8; 3]) {
        for x in x1.max(0)..=x2.min(self.width - 1) {
            self.pixel(x, y, color);
        }
    }
    pub fn rect(&mut self, rect: Rect, color: [u8; 3], border: bool) {
        if rect.width <= 0 || rect.height <= 0 {
            return;
        }
        let x2 = rect.x + rect.width - 1;
        let y2 = rect.y + rect.height - 1;
        if border {
            self.horizontal(rect.x, x2, rect.y, color);
            self.horizontal(rect.x, x2, y2, color);
            for y in rect.y.max(0)..=y2.min(self.height - 1) {
                self.pixel(rect.x, y, color);
                self.pixel(x2, y, color);
            }
        } else {
            for y in rect.y.max(0)..=y2.min(self.height - 1) {
                self.horizontal(rect.x, x2, y, color);
            }
        }
    }
    pub fn line2(&mut self, start: (i32, i32), end: (i32, i32), color: [u8; 3]) {
        let (mut x, mut y) = start;
        let (x2, y2) = end;
        let dx = (x2 - x).abs();
        let dy = (y2 - y).abs();
        let sx = if x < x2 { 1 } else { -1 };
        let sy = if y < y2 { 1 } else { -1 };
        let xinc = dx <= dy;
        let mut err = if dx > dy { dx } else { -dy } / 2;
        loop {
            if xinc {
                self.pixel(x, y, color);
                self.pixel(x + 1, y, color);
            } else {
                self.pixel(x, y, color);
                self.pixel(x, y + 1, color);
            }
            if x == x2 && y == y2 {
                break;
            }
            let e = err;
            if e > -dx {
                err -= dy;
                x += sx;
            }
            if e < dy {
                err += dx;
                y += sy;
            }
        }
    }
    pub fn circle9(&mut self, center: (i32, i32), color: [u8; 3]) {
        let (x0, y0) = center;
        let mut f = -8;
        let mut ddx = 0;
        let mut ddy = -18;
        let mut x = 0;
        let mut y = 9;
        while x < y {
            if f >= 0 {
                y -= 1;
                ddy += 2;
                f += ddy;
            }
            x += 1;
            ddx += 2;
            f += ddx + 1;
            if f >= 0 {
                self.horizontal(x0 - x, x0 + x - 1, y0 + y - 1, color);
                self.horizontal(x0 - x, x0 + x - 1, y0 - y, color);
            }
            self.horizontal(x0 - y, x0 + y - 1, y0 + x - 1, color);
            self.horizontal(x0 - y, x0 + y - 1, y0 - x, color);
        }
    }
    pub fn blit(&mut self, image: &Image, x: i32, y: i32) {
        let Ok(width) = usize::try_from(image.width) else {
            return;
        };
        let Ok(canvas_width) = usize::try_from(self.width) else {
            return;
        };
        for (index, pixel) in image.rgba.chunks_exact(4).enumerate() {
            let Ok(sx) = i32::try_from(index % width.max(1)) else {
                continue;
            };
            let Ok(sy) = i32::try_from(index / width.max(1)) else {
                continue;
            };
            let dx = x + sx;
            let dy = y + sy;
            if dx < 0 || dy < 0 || dx >= self.width || dy >= self.height {
                continue;
            }
            let (Ok(dx), Ok(dy)) = (usize::try_from(dx), usize::try_from(dy)) else {
                continue;
            };
            let out = &mut self.rgba[(dy * canvas_width + dx) * 4..][..4];
            for channel in 0..3 {
                let src = i32::from(pixel[channel]);
                let dst = i32::from(out[channel]);
                let value = (((src - dst) * i32::from(pixel[3]) + src) >> 8) + dst;
                if let Ok(value) = u8::try_from(value) {
                    out[channel] = value;
                }
            }
        }
    }
}
