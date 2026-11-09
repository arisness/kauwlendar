use crate::calendar::CalendarEvent;
use crate::config::TimeFormat;
use crate::ui::canvas::{
    Canvas, COLOR_BLACK, COLOR_DARK_GRAY, COLOR_GRAY, COLOR_LIGHT_GRAY, COLOR_WHITE,
};
use crate::ui::fonts::{font_bold, font_regular};
use chrono::{DateTime, Datelike, FixedOffset, Timelike};

pub fn render_dashboard(
    canvas: &mut Canvas,
    now: &DateTime<FixedOffset>,
    events: &[CalendarEvent],
    time_format: TimeFormat,
    battery_level: Option<u32>,
    _refresh_minutes: u32,
) {
    canvas.fill(COLOR_WHITE);

    let reg = font_regular();
    let bold = font_bold();

    // Factor de escala relativo a la resolución base (1072x1448)
    let scale = (canvas.width as f32 / 1072.0).clamp(0.6, 1.4);

    let margin = (42.0 * scale).round() as u32;
    let header_h = (210.0 * scale).round() as u32;

    // 1. BARRA SUPERIOR (HEADER) EN ALTO CONTRASTE
    canvas.draw_rect_fill(0, 0, canvas.width, header_h, COLOR_BLACK);

    // Fecha en español (grande, blanca pura y en negrita)
    let date_str = format_date_spanish(now);
    let date_font_sz = (42.0 * scale).round();
    let date_y = (30.0 * scale).round() as i32;
    canvas.draw_text(&bold, &date_str, margin as i32, date_y, date_font_sz, COLOR_WHITE);

    // Reloj principal gigante (92px)
    let time_str = format_current_time(now, time_format);
    let time_font_sz = (92.0 * scale).round();
    let time_y = (86.0 * scale).round() as i32;
    canvas.draw_text(&bold, &time_str, margin as i32, time_y, time_font_sz, COLOR_WHITE);

    // Botón [SALIR] grande y cómodo en la esquina superior derecha
    let exit_btn_w = (220.0 * scale).round() as u32;
    let exit_btn_h = (88.0 * scale).round() as u32;
    let exit_btn_x = canvas.width.saturating_sub(exit_btn_w + margin);
    let exit_btn_y = (60.0 * scale).round() as u32;

    canvas.draw_rounded_rect_fill(exit_btn_x, exit_btn_y, exit_btn_w, exit_btn_h, 14, COLOR_WHITE);
    let exit_font_sz = (36.0 * scale).round();
    let (lbl_w, _) = Canvas::measure_text(&bold, "SALIR", exit_font_sz);
    let lbl_x = exit_btn_x as i32 + ((exit_btn_w as f32 - lbl_w) / 2.0) as i32;
    let lbl_y = exit_btn_y as i32 + ((exit_btn_h as f32 - exit_font_sz) / 2.0) as i32;
    canvas.draw_text(&bold, "SALIR", lbl_x, lbl_y, exit_font_sz, COLOR_BLACK);

    // Indicador de batería grande y muy legible (42px)
    if let Some(batt) = battery_level {
        let batt_str = format!("{}%", batt);
        let batt_font_sz = (42.0 * scale).round();
        let (b_w, _) = Canvas::measure_text(&bold, &batt_str, batt_font_sz);
        let batt_box_w = (b_w + 36.0 * scale).round() as u32;
        let batt_box_h = (70.0 * scale).round() as u32;
        let batt_box_x = exit_btn_x.saturating_sub(batt_box_w + (20.0 * scale) as u32);
        let batt_box_y = (69.0 * scale).round() as u32;

        canvas.draw_rounded_rect_fill(batt_box_x, batt_box_y, batt_box_w, batt_box_h, 10, COLOR_DARK_GRAY);
        let b_text_x = batt_box_x as i32 + ((batt_box_w as f32 - b_w) / 2.0) as i32;
        let b_text_y = batt_box_y as i32 + ((batt_box_h as f32 - batt_font_sz) / 2.0) as i32;
        canvas.draw_text(&bold, &batt_str, b_text_x, b_text_y, batt_font_sz, COLOR_WHITE);
    }

    // 2. CUERPO DE EVENTOS (TARJETAS GRANDES Y ESPACIOSAS)
    let mut current_y = header_h + (32.0 * scale).round() as u32;
    let content_w = canvas.width.saturating_sub(2 * margin);

    let (all_day_events, timed_events): (Vec<&CalendarEvent>, Vec<&CalendarEvent>) =
        events.iter().partition(|e| e.is_all_day);

    let total_events = all_day_events.len() + timed_events.len();

    // Altura adaptativa de tarjeta según la cantidad de eventos para aprovechar la pantalla
    let card_h = match total_events {
        0..=2 => (240.0 * scale).round() as u32,
        3 => (220.0 * scale).round() as u32,
        4 => (195.0 * scale).round() as u32,
        5 => (160.0 * scale).round() as u32,
        _ => (135.0 * scale).round() as u32,
    };
    let card_gap = match total_events {
        0..=3 => (24.0 * scale).round() as u32,
        4 => (20.0 * scale).round() as u32,
        _ => (14.0 * scale).round() as u32,
    };

    let section_title_sz = (34.0 * scale).round();

    if !all_day_events.is_empty() {
        canvas.draw_text(&bold, "TODO EL DÍA", margin as i32, current_y as i32, section_title_sz, COLOR_DARK_GRAY);
        current_y += (42.0 * scale).round() as u32;
        canvas.draw_line_h(margin, current_y, content_w, 2, COLOR_LIGHT_GRAY);
        current_y += (18.0 * scale).round() as u32;

        for ev in all_day_events {
            canvas.draw_rounded_rect_fill(margin, current_y, content_w, card_h, 14, COLOR_LIGHT_GRAY);
            canvas.draw_rect_outline(margin, current_y, content_w, card_h, 2, COLOR_DARK_GRAY);

            // Fila 1: Pastillas de estado
            let badge_h = (48.0 * scale).round() as u32;
            let badge_y = current_y + (22.0 * scale) as u32;

            let all_day_lbl = "TODO EL DÍA";
            let ad_sz = (28.0 * scale).round();
            let (ad_w, _) = Canvas::measure_text(&bold, all_day_lbl, ad_sz);
            let ad_box_w = (ad_w + 30.0 * scale).round() as u32;
            let ad_box_x = margin + (18.0 * scale) as u32;
            canvas.draw_rounded_rect_fill(ad_box_x, badge_y, ad_box_w, badge_h, 8, COLOR_BLACK);
            canvas.draw_text(&bold, all_day_lbl, (ad_box_x as f32 + 15.0 * scale) as i32, (badge_y as f32 + (badge_h as f32 - ad_sz) / 2.0) as i32, ad_sz, COLOR_WHITE);

            let cal_sz = (26.0 * scale).round();
            let cal_name = truncate_text(&ev.calendar_name, 16);
            let (c_w, _) = Canvas::measure_text(&bold, &cal_name, cal_sz);
            let cal_box_w = (c_w + 26.0 * scale).round() as u32;
            let cal_box_x = ad_box_x + ad_box_w + (14.0 * scale) as u32;
            canvas.draw_rounded_rect_fill(cal_box_x, badge_y, cal_box_w, badge_h, 8, COLOR_DARK_GRAY);
            canvas.draw_text(&bold, &cal_name, (cal_box_x as f32 + 13.0 * scale) as i32, (badge_y as f32 + (badge_h as f32 - cal_sz) / 2.0) as i32, cal_sz, COLOR_WHITE);

            // Fila 2: Título de evento gigante (48px)
            let summary_sz = (48.0 * scale).round();
            let summary_x = margin + (22.0 * scale) as u32;
            let summary_y = badge_y + badge_h + (24.0 * scale) as u32;
            let summary_text = truncate_text(&ev.summary, 40);
            canvas.draw_text(&bold, &summary_text, summary_x as i32, summary_y as i32, summary_sz, COLOR_BLACK);

            current_y += card_h + card_gap;
        }

        current_y += (15.0 * scale).round() as u32;
    }

    // Título de sección "AGENDA DE HOY"
    canvas.draw_text(&bold, "AGENDA DE HOY", margin as i32, current_y as i32, section_title_sz, COLOR_BLACK);
    current_y += (42.0 * scale).round() as u32;
    canvas.draw_line_h(margin, current_y, content_w, 3, COLOR_BLACK);
    current_y += (22.0 * scale).round() as u32;

    let footer_h = (65.0 * scale).round() as u32;
    let max_y = canvas.height.saturating_sub(footer_h + (20.0 * scale) as u32);

    if timed_events.is_empty() && events.is_empty() {
        let box_y = current_y + (40.0 * scale).round() as u32;
        let box_h = (240.0 * scale).round() as u32;
        canvas.draw_rounded_rect_fill(margin, box_y, content_w, box_h, 16, COLOR_LIGHT_GRAY);
        canvas.draw_rect_outline(margin, box_y, content_w, box_h, 3, COLOR_GRAY);

        let msg1 = "Sin eventos programados para hoy";
        let msg1_sz = (38.0 * scale).round();
        let (m1_w, _) = Canvas::measure_text(&bold, msg1, msg1_sz);
        let m1_x = (canvas.width as f32 - m1_w) / 2.0;
        canvas.draw_text(&bold, msg1, m1_x as i32, (box_y + (70.0 * scale) as u32) as i32, msg1_sz, COLOR_BLACK);

        let msg2 = "Disfruta de tu día libre o añade eventos a tus calendarios.";
        let msg2_sz = (26.0 * scale).round();
        let (m2_w, _) = Canvas::measure_text(&reg, msg2, msg2_sz);
        let m2_x = (canvas.width as f32 - m2_w) / 2.0;
        canvas.draw_text(&reg, msg2, m2_x as i32, (box_y + (140.0 * scale) as u32) as i32, msg2_sz, COLOR_DARK_GRAY);
    } else {
        for ev in timed_events {
            if current_y + card_h > max_y {
                canvas.draw_text(&bold, "... y más eventos restantes", (margin + 10) as i32, current_y as i32, (26.0 * scale).round(), COLOR_DARK_GRAY);
                break;
            }

            canvas.draw_rounded_rect_fill(margin, current_y, content_w, card_h, 14, COLOR_LIGHT_GRAY);
            canvas.draw_rect_outline(margin, current_y, content_w, card_h, 2, COLOR_DARK_GRAY);

            // Fila 1: Pastillas de Rango de Hora y Calendario
            let badge_h = (48.0 * scale).round() as u32;
            let badge_y = current_y + (22.0 * scale) as u32;

            // Pastilla de hora negra de alto contraste (muestra inicio - fin si está disponible)
            let time_label = ev.time_label(time_format);
            let t_sz = (30.0 * scale).round();
            let (t_w, _) = Canvas::measure_text(&bold, &time_label, t_sz);
            let time_box_w = (t_w + 32.0 * scale).round() as u32;
            let time_box_x = margin + (18.0 * scale) as u32;
            canvas.draw_rounded_rect_fill(time_box_x, badge_y, time_box_w, badge_h, 8, COLOR_BLACK);
            canvas.draw_text(&bold, &time_label, (time_box_x as f32 + 16.0 * scale) as i32, (badge_y as f32 + (badge_h as f32 - t_sz) / 2.0) as i32, t_sz, COLOR_WHITE);

            // Pastilla de calendario
            let cal_sz = (26.0 * scale).round();
            let cal_name = truncate_text(&ev.calendar_name, 16);
            let (c_w, _) = Canvas::measure_text(&bold, &cal_name, cal_sz);
            let cal_box_w = (c_w + 26.0 * scale).round() as u32;
            let cal_box_x = time_box_x + time_box_w + (14.0 * scale) as u32;
            canvas.draw_rounded_rect_fill(cal_box_x, badge_y, cal_box_w, badge_h, 8, COLOR_DARK_GRAY);
            canvas.draw_text(&bold, &cal_name, (cal_box_x as f32 + 13.0 * scale) as i32, (badge_y as f32 + (badge_h as f32 - cal_sz) / 2.0) as i32, cal_sz, COLOR_WHITE);

            // Fila 2: Título del evento gigante (48px negrita)
            let summary_sz = (48.0 * scale).round();
            let summary_x = margin + (22.0 * scale) as u32;
            let summary_y = badge_y + badge_h + (24.0 * scale) as u32;
            let summary_text = truncate_text(&ev.summary, 40);
            canvas.draw_text(&bold, &summary_text, summary_x as i32, summary_y as i32, summary_sz, COLOR_BLACK);

            current_y += card_h + card_gap;
        }
    }

    // 3. BARRA INFERIOR (FOOTER MINIMALISTA Y LIMPIO)
    let footer_y = canvas.height.saturating_sub(footer_h);
    canvas.draw_line_h(margin, footer_y, content_w, 2, COLOR_LIGHT_GRAY);

    let exit_hint = "Toca [SALIR] para volver a Kindle";
    let footer_sz = (24.0 * scale).round();
    let (hint_w, _) = Canvas::measure_text(&reg, exit_hint, footer_sz);
    let hint_x = (canvas.width as f32 - hint_w) / 2.0;
    canvas.draw_text(&reg, exit_hint, hint_x as i32, (footer_y + (22.0 * scale) as u32) as i32, footer_sz, COLOR_DARK_GRAY);
}

fn format_current_time(now: &DateTime<FixedOffset>, format: TimeFormat) -> String {
    let hour = now.hour();
    let minute = now.minute();
    match format {
        TimeFormat::H24 => format!("{:02}:{:02}", hour, minute),
        TimeFormat::H12 => {
            let (h12, period) = match hour {
                0 => (12, "AM"),
                1..=11 => (hour, "AM"),
                12 => (12, "PM"),
                _ => (hour - 12, "PM"),
            };
            format!("{:02}:{:02} {}", h12, minute, period)
        }
    }
}

fn format_date_spanish(dt: &DateTime<FixedOffset>) -> String {
    let day_name = match dt.weekday() {
        chrono::Weekday::Mon => "Lunes",
        chrono::Weekday::Tue => "Martes",
        chrono::Weekday::Wed => "Miércoles",
        chrono::Weekday::Thu => "Jueves",
        chrono::Weekday::Fri => "Viernes",
        chrono::Weekday::Sat => "Sábado",
        chrono::Weekday::Sun => "Domingo",
    };

    let month_name = match dt.month() {
        1 => "Enero", 2 => "Febrero", 3 => "Marzo", 4 => "Abril",
        5 => "Mayo", 6 => "Junio", 7 => "Julio", 8 => "Agosto",
        9 => "Septiembre", 10 => "Octubre", 11 => "Noviembre", 12 => "Diciembre",
        _ => "",
    };

    format!("{}, {} de {} de {}", day_name, dt.day(), month_name, dt.year())
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max_chars.saturating_sub(3)).collect();
        s.push_str("...");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    #[test]
    fn test_render_dashboard_large_layout() {
        let mut canvas = Canvas::new(1072, 1448);
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 8)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(19, 54, 0).unwrap())
            .and_local_timezone(offset)
            .unwrap();

        let events = vec![
            CalendarEvent::new_timed("Personal", "Event 1", 6 * 60)
                .with_end_minutes(7 * 60 + 45)
                .with_description("Morning routine notes"),
            CalendarEvent::new_timed("Work", "Event 2", 9 * 60)
                .with_end_minutes(10 * 60 + 30),
        ];

        render_dashboard(&mut canvas, &now, &events, TimeFormat::H12, Some(89), 30);
        assert_eq!(canvas.width, 1072);
        assert_eq!(canvas.height, 1448);
    }
}
