use ab_glyph::{Font, Point, PxScale, ScaleFont};
use image::{GrayImage, Luma};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[allow(dead_code)]
pub const KINDLE_WIDTH: u32 = 1072;
#[allow(dead_code)]
pub const KINDLE_HEIGHT: u32 = 1448;

pub const COLOR_WHITE: u8 = 255;
pub const COLOR_LIGHT_GRAY: u8 = 220;
pub const COLOR_GRAY: u8 = 160;
pub const COLOR_DARK_GRAY: u8 = 80;
pub const COLOR_BLACK: u8 = 0;

pub struct Canvas {
    pub img: GrayImage,
    pub width: u32,
    pub height: u32,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let img = GrayImage::from_pixel(width, height, Luma([COLOR_WHITE]));
        Self { img, width, height }
    }

    pub fn default_kindle() -> Self {
        let (w, h) = crate::platform::get_screen_dimensions();
        Self::new(w, h)
    }

    pub fn fill(&mut self, color: u8) {
        for pixel in self.img.pixels_mut() {
            *pixel = Luma([color]);
        }
    }

    pub fn draw_rect_fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: u8) {
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);

        for py in y..y_end {
            for px in x..x_end {
                self.img.put_pixel(px, py, Luma([color]));
            }
        }
    }

    pub fn draw_rect_outline(&mut self, x: u32, y: u32, w: u32, h: u32, thickness: u32, color: u8) {
        if w == 0 || h == 0 {
            return;
        }

        // Líneas horizontales (superior e inferior)
        self.draw_rect_fill(x, y, w, thickness, color);
        self.draw_rect_fill(x, y + h.saturating_sub(thickness), w, thickness, color);

        // Líneas verticales (izquierda y derecha)
        self.draw_rect_fill(x, y, thickness, h, color);
        self.draw_rect_fill(x + w.saturating_sub(thickness), y, thickness, h, color);
    }

    pub fn draw_rounded_rect_fill(&mut self, x: u32, y: u32, w: u32, h: u32, radius: u32, color: u8) {
        let r = radius.min(w / 2).min(h / 2);
        // Rectángulo central
        self.draw_rect_fill(x + r, y, w.saturating_sub(2 * r), h, color);
        // Alas laterales
        self.draw_rect_fill(x, y + r, r, h.saturating_sub(2 * r), color);
        self.draw_rect_fill(x + w.saturating_sub(r), y + r, r, h.saturating_sub(2 * r), color);

        // Esquinas circulares
        self.draw_circle_corner(x + r, y + r, r, 2, color); // Sup izq
        self.draw_circle_corner(x + w - r, y + r, r, 1, color); // Sup der
        self.draw_circle_corner(x + r, y + h - r, r, 3, color); // Inf izq
        self.draw_circle_corner(x + w - r, y + h - r, r, 4, color); // Inf der
    }

    fn draw_circle_corner(&mut self, cx: u32, cy: u32, radius: u32, quadrant: u8, color: u8) {
        let r_i32 = radius as i32;
        let r_sq = r_i32 * r_i32;

        for dy in 0..=r_i32 {
            for dx in 0..=r_i32 {
                if dx * dx + dy * dy <= r_sq {
                    let (px, py) = match quadrant {
                        1 => (cx as i32 + dx, cy as i32 - dy),
                        2 => (cx as i32 - dx, cy as i32 - dy),
                        3 => (cx as i32 - dx, cy as i32 + dy),
                        4 => (cx as i32 + dx, cy as i32 + dy),
                        _ => (cx as i32, cy as i32),
                    };

                    if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                        self.img.put_pixel(px as u32, py as u32, Luma([color]));
                    }
                }
            }
        }
    }

    pub fn draw_line_h(&mut self, x: u32, y: u32, length: u32, thickness: u32, color: u8) {
        self.draw_rect_fill(x, y, length, thickness, color);
    }

    pub fn draw_line_v(&mut self, x: u32, y: u32, length: u32, thickness: u32, color: u8) {
        self.draw_rect_fill(x, y, thickness, length, color);
    }

    pub fn draw_circle_fill(&mut self, cx: i32, cy: i32, radius: i32, color: u8) {
        let r_sq = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r_sq {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                        self.img.put_pixel(px as u32, py as u32, Luma([color]));
                    }
                }
            }
        }
    }

    pub fn draw_text<F: Font>(&mut self, font: &F, text: &str, x: i32, y: i32, scale_px: f32, color: u8) {
        let scaled_font = font.as_scaled(PxScale::from(scale_px));
        let mut caret = Point {
            x: x as f32,
            y: y as f32 + scaled_font.ascent(),
        };

        for ch in text.chars() {
            let mut glyph = scaled_font.scaled_glyph(ch);
            glyph.position = caret;
            let advance_width = scaled_font.h_advance(glyph.id);
            caret.x += advance_width;

            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, alpha| {
                    if alpha <= 0.05 {
                        return;
                    }
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;

                    if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                        let current = self.img.get_pixel(px as u32, py as u32)[0] as f32;
                        let target = color as f32;
                        let blended = (current * (1.0 - alpha) + target * alpha).round() as u8;
                        self.img.put_pixel(px as u32, py as u32, Luma([blended]));
                    }
                });
            }
        }
    }

    pub fn measure_text<F: Font>(font: &F, text: &str, scale_px: f32) -> (f32, f32) {
        let scaled_font = font.as_scaled(PxScale::from(scale_px));
        let mut width = 0.0;
        for ch in text.chars() {
            let glyph = scaled_font.scaled_glyph(ch);
            width += scaled_font.h_advance(glyph.id);
        }
        let height = scaled_font.ascent() - scaled_font.descent();
        (width, height)
    }

    pub fn save_png<P: AsRef<Path>>(&self, path: P) -> Result<(), image::ImageError> {
        self.img.save(path)
    }

    pub fn render_to_screen(&self) -> Result<(), String> {
        let is_kindle = crate::platform::is_kindle();
        let target_path = if is_kindle {
            "/tmp/kauwlendar.png"
        } else {
            "preview.png"
        };

        self.save_png(target_path)
            .map_err(|e| format!("Error guardando imagen en {}: {}", target_path, e))?;

        if is_kindle {
            // Pausa breve para asegurar que cualquier salida previa del lanzador KPM haya terminado
            thread::sleep(Duration::from_millis(150));

            let cmd_bin = crate::platform::find_fbink_bin();

            // Renderizar a tamaño 100% del viewport visible (w=-1,h=-1) con refresco completo (-f)
            // Silenciando stdout/stderr para que el visor de logs de KPM no sobreescriba la pantalla
            let file_arg_fit = format!("file={},w=-1,h=-1", target_path);

            let ok = Command::new(cmd_bin)
                .args(["-q", "-c", "-f", "-g", &file_arg_fit])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if !ok {
                // Segundo intento sin escalado forzado (ya tiene el tamaño nativo de la pantalla)
                let file_arg_native = format!("file={}", target_path);
                let ok_native = Command::new(cmd_bin)
                    .args(["-q", "-c", "-f", "-g", &file_arg_native])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !ok_native {
                    // Fallback a eips si fbink falla
                    for eips_bin in &["/usr/sbin/eips", "/usr/bin/eips", "eips"] {
                        let _ = Command::new(eips_bin)
                            .arg("-c")
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                        if Command::new(eips_bin)
                            .args(["-f", "-g", target_path])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                            .is_ok()
                        {
                            break;
                        }
                    }
                }
            }
        } else {
            println!("╔════════════════════════════════════════════════════════════════════╗");
            println!("║  [DEV] Vista previa guardada en: ./preview.png                      ║");
            println!("║  Abre preview.png para ver el diseño exacto que verá tu Kindle.    ║");
            println!("╚════════════════════════════════════════════════════════════════════╝");
        }

        Ok(())
    }
}
