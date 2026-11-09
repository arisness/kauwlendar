use crate::ui::canvas::{
    Canvas, COLOR_BLACK, COLOR_DARK_GRAY, COLOR_GRAY, COLOR_LIGHT_GRAY, COLOR_WHITE,
};
use crate::ui::fonts::{font_bold, font_regular};

pub fn render_error_screen(canvas: &mut Canvas, title: &str, details: &str, suggestion: &str) {
    canvas.fill(COLOR_WHITE);

    let reg = font_regular();
    let bold = font_bold();

    let scale = (canvas.width as f32 / 1072.0).clamp(0.55, 1.2);
    let margin = (40.0 * scale).round() as u32;
    let header_h = (130.0 * scale).round() as u32;

    // 1. Barra superior de alerta (negra)
    canvas.draw_rect_fill(0, 0, canvas.width, header_h, COLOR_BLACK);
    let h_sz = (40.0 * scale).round();
    let h_y = (header_h as f32 - h_sz) / 2.0;
    canvas.draw_text(&bold, "⚠️ ERROR EN KAUWLENDAR", margin as i32, h_y as i32, h_sz, COLOR_WHITE);

    // 2. Tarjeta principal de error
    let content_w = canvas.width.saturating_sub(2 * margin);
    let box_y = header_h + (40.0 * scale).round() as u32;
    let box_h = (canvas.height as f32 * 0.45).round() as u32;

    canvas.draw_rounded_rect_fill(margin, box_y, content_w, box_h, 14, COLOR_LIGHT_GRAY);
    canvas.draw_rect_outline(margin, box_y, content_w, box_h, 3, COLOR_BLACK);

    // Título del problema
    let t_sz = (32.0 * scale).round();
    canvas.draw_text(&bold, title, (margin + (30.0 * scale) as u32) as i32, (box_y + (40.0 * scale) as u32) as i32, t_sz, COLOR_BLACK);
    canvas.draw_line_h(margin + (30.0 * scale) as u32, box_y + (85.0 * scale) as u32, content_w.saturating_sub((60.0 * scale) as u32), 2, COLOR_GRAY);

    // Mensaje de detalle
    let mut detail_y = box_y + (110.0 * scale).round() as u32;
    let text_sz = (22.0 * scale).round();
    let max_chars = ((content_w as f32) / (text_sz * 0.60)) as usize;

    for line in wrap_lines(details, max_chars) {
        if detail_y > box_y + box_h - (120.0 * scale) as u32 {
            break;
        }
        canvas.draw_text(&reg, &line, (margin + (30.0 * scale) as u32) as i32, detail_y as i32, text_sz, COLOR_DARK_GRAY);
        detail_y += (32.0 * scale).round() as u32;
    }

    // Sugerencia
    if !suggestion.is_empty() {
        detail_y += (20.0 * scale).round() as u32;
        canvas.draw_text(&bold, "Sugerencia:", (margin + (30.0 * scale) as u32) as i32, detail_y as i32, text_sz, COLOR_BLACK);
        detail_y += (30.0 * scale).round() as u32;
        for line in wrap_lines(suggestion, max_chars) {
            if detail_y > box_y + box_h - (40.0 * scale) as u32 {
                break;
            }
            canvas.draw_text(&reg, &line, (margin + (30.0 * scale) as u32) as i32, detail_y as i32, text_sz - 2.0, COLOR_DARK_GRAY);
            detail_y += (28.0 * scale).round() as u32;
        }
    }

    // 3. Botón de SALIR (grande, centrado y fácil de tocar)
    let btn_w = (420.0 * scale).round() as u32;
    let btn_h = (90.0 * scale).round() as u32;
    let btn_x = (canvas.width.saturating_sub(btn_w)) / 2;
    let btn_y = box_y + box_h + (50.0 * scale).round() as u32;

    canvas.draw_rounded_rect_fill(btn_x, btn_y, btn_w, btn_h, 14, COLOR_BLACK);
    let btn_label = "SALIR AL KINDLE";
    let btn_lbl_sz = (32.0 * scale).round();
    let (lbl_w, _) = Canvas::measure_text(&bold, btn_label, btn_lbl_sz);
    let lbl_x = btn_x as f32 + (btn_w as f32 - lbl_w) / 2.0;
    let lbl_y = btn_y as f32 + (btn_h as f32 - btn_lbl_sz) / 2.0;
    canvas.draw_text(&bold, btn_label, lbl_x as i32, lbl_y as i32, btn_lbl_sz, COLOR_WHITE);

    // Subtítulo de instrucción
    let exit_sub = "Toca la pantalla para volver al menú de Kindle";
    let sub_sz = (22.0 * scale).round();
    let (sub_w, _) = Canvas::measure_text(&reg, exit_sub, sub_sz);
    let sub_x = (canvas.width as f32 - sub_w) / 2.0;
    canvas.draw_text(&reg, exit_sub, sub_x as i32, (btn_y + btn_h + (35.0 * scale) as u32) as i32, sub_sz, COLOR_DARK_GRAY);
}

fn wrap_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        let mut current_line = String::new();
        for word in words {
            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.chars().count() + 1 + word.chars().count() <= max_chars.max(10) {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    lines
}
