use crate::calendar::CalendarEvent;
use crate::config::{Language, TimeFormat};
use crate::ui::canvas::{
    Canvas, COLOR_BLACK, COLOR_DARK_GRAY, COLOR_GRAY, COLOR_LIGHT_GRAY, COLOR_WHITE,
};
use crate::ui::fonts::{font_bold, font_regular};
use crate::ui::i18n::get_translations;
use ab_glyph::Font;
use chrono::{DateTime, FixedOffset, Timelike};

pub fn render_dashboard(
    canvas: &mut Canvas,
    now: &DateTime<FixedOffset>,
    events: &[CalendarEvent],
    start_idx: usize,
    time_format: TimeFormat,
    battery_level: Option<u32>,
    language: Language,
) {
    canvas.fill(COLOR_WHITE);

    let reg = font_regular();
    let bold = font_bold();
    let i18n = get_translations(language);

    // Factor de escala relativo a la resolución base (1072x1448)
    let scale = (canvas.width as f32 / 1072.0).clamp(0.6, 1.4);

    let margin = (42.0 * scale).round() as u32;
    let header_h = (210.0 * scale).round() as u32;

    // 1. BARRA SUPERIOR (HEADER) EN ALTO CONTRASTE
    canvas.draw_rect_fill(0, 0, canvas.width, header_h, COLOR_BLACK);

    // Fecha en el idioma configurado (grande, blanca pura y en negrita)
    let date_str = (i18n.format_date)(now);
    let date_font_sz = (42.0 * scale).round();
    let date_y = (30.0 * scale).round() as i32;
    canvas.draw_text(&bold, &date_str, margin as i32, date_y, date_font_sz, COLOR_WHITE);

    // Reloj principal gigante (92px)
    let time_str = format_current_time(now, time_format);
    let time_font_sz = (92.0 * scale).round();
    let time_y = (86.0 * scale).round() as i32;
    canvas.draw_text(&bold, &time_str, margin as i32, time_y, time_font_sz, COLOR_WHITE);

    // Botón [SALIR] / [EXIT] grande y cómodo en la esquina superior derecha
    let exit_btn_w = (220.0 * scale).round() as u32;
    let exit_btn_h = (88.0 * scale).round() as u32;
    let exit_btn_x = canvas.width.saturating_sub(exit_btn_w + margin);
    let exit_btn_y = (60.0 * scale).round() as u32;

    canvas.draw_rounded_rect_fill(exit_btn_x, exit_btn_y, exit_btn_w, exit_btn_h, 14, COLOR_WHITE);
    let exit_font_sz = (36.0 * scale).round();
    let (lbl_w, _) = Canvas::measure_text(&bold, i18n.exit_button, exit_font_sz);
    let lbl_x = exit_btn_x as i32 + ((exit_btn_w as f32 - lbl_w) / 2.0) as i32;
    let lbl_y = exit_btn_y as i32 + ((exit_btn_h as f32 - exit_font_sz) / 2.0) as i32;
    canvas.draw_text(&bold, i18n.exit_button, lbl_x, lbl_y, exit_font_sz, COLOR_BLACK);

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

    // 2. CUERPO DE EVENTOS (VENTANA ACTIVA DE HASTA 4 EVENTOS)
    let total_events = events.len();
    let page_size = 4;
    let visible_events: &[CalendarEvent] = if total_events <= page_size {
        events
    } else {
        let start = start_idx.min(total_events.saturating_sub(page_size));
        let end = (start + page_size).min(total_events);
        &events[start..end]
    };

    let mut current_y = header_h + (28.0 * scale).round() as u32;
    let content_w = canvas.width.saturating_sub(2 * margin);
    let footer_h = (65.0 * scale).round() as u32;
    let footer_y = canvas.height.saturating_sub(footer_h);

    let (all_day_events, timed_events): (Vec<&CalendarEvent>, Vec<&CalendarEvent>) =
        visible_events.iter().partition(|e| e.is_all_day);

    // Adaptive card height using remaining vertical space for up to 4 cards
    let layout = compute_card_layout(
        current_y,
        footer_y,
        all_day_events.len(),
        timed_events.len(),
        scale,
    );

    let section_title_sz = (34.0 * scale).round();

    if !all_day_events.is_empty() {
        canvas.draw_text(
            &bold,
            i18n.all_day_header,
            margin as i32,
            current_y as i32,
            section_title_sz,
            COLOR_DARK_GRAY,
        );
        current_y += (42.0 * scale).round() as u32;
        canvas.draw_line_h(margin, current_y, content_w, 2, COLOR_LIGHT_GRAY);
        current_y += (18.0 * scale).round() as u32;

        let all_day_len = all_day_events.len();
        for (idx, ev) in all_day_events.iter().enumerate() {
            draw_event_card(
                canvas,
                margin,
                current_y,
                content_w,
                &layout,
                i18n.all_day_badge,
                layout.all_day_font_sz,
                &ev.calendar_name,
                &ev.summary,
                &bold,
                scale,
            );
            current_y += layout.card_h;
            if idx + 1 < all_day_len {
                current_y += layout.card_gap;
            }
        }

        if !timed_events.is_empty() {
            current_y += (22.0 * scale).round() as u32;
        }
    }

    // Título de sección "AGENDA DE HOY" / "TODAY'S AGENDA"
    if !timed_events.is_empty() || events.is_empty() {
        canvas.draw_text(
            &bold,
            i18n.today_agenda_header,
            margin as i32,
            current_y as i32,
            section_title_sz,
            COLOR_BLACK,
        );
        current_y += (42.0 * scale).round() as u32;
        canvas.draw_line_h(margin, current_y, content_w, 3, COLOR_BLACK);
        current_y += (22.0 * scale).round() as u32;
    }

    if timed_events.is_empty() && events.is_empty() {
        let box_h = (280.0 * scale).round() as u32;
        let box_y = current_y + (40.0 * scale).round() as u32;
        canvas.draw_rounded_rect_fill(margin, box_y, content_w, box_h, 16, COLOR_LIGHT_GRAY);
        canvas.draw_rect_outline(margin, box_y, content_w, box_h, 3, COLOR_GRAY);

        let msg1 = i18n.no_events_title;
        let msg1_sz = (44.0 * scale).round();
        let (m1_w, _) = Canvas::measure_text(&bold, msg1, msg1_sz);
        let m1_x = (canvas.width as f32 - m1_w) / 2.0;
        canvas.draw_text(
            &bold,
            msg1,
            m1_x as i32,
            (box_y + (82.0 * scale) as u32) as i32,
            msg1_sz,
            COLOR_BLACK,
        );

        let msg2 = i18n.no_events_subtitle;
        let msg2_sz = (30.0 * scale).round();
        let (m2_w, _) = Canvas::measure_text(&reg, msg2, msg2_sz);
        let m2_x = (canvas.width as f32 - m2_w) / 2.0;
        canvas.draw_text(
            &reg,
            msg2,
            m2_x as i32,
            (box_y + (164.0 * scale) as u32) as i32,
            msg2_sz,
            COLOR_DARK_GRAY,
        );
    } else {
        let timed_len = timed_events.len();
        for (idx, ev) in timed_events.iter().enumerate() {
            let time_label = ev.time_label(time_format);
            draw_event_card(
                canvas,
                margin,
                current_y,
                content_w,
                &layout,
                &time_label,
                layout.time_font_sz,
                &ev.calendar_name,
                &ev.summary,
                &bold,
                scale,
            );
            current_y += layout.card_h;
            if idx + 1 < timed_len {
                current_y += layout.card_gap;
            }
        }
    }

    // 3. BARRA INFERIOR (FOOTER / INDICADOR DE PAGINACIÓN SIN BOTONES)
    canvas.draw_line_h(margin, footer_y, content_w, 2, COLOR_LIGHT_GRAY);

    let footer_sz = (24.0 * scale).round();
    let text_y = (footer_y + (22.0 * scale) as u32) as i32;

    if total_events <= page_size {
        let exit_hint = i18n.footer_hint;
        let (hint_w, _) = Canvas::measure_text(&reg, exit_hint, footer_sz);
        let hint_x = (canvas.width as f32 - hint_w) / 2.0;
        canvas.draw_text(&reg, exit_hint, hint_x as i32, text_y, footer_sz, COLOR_DARK_GRAY);
    } else {
        let (current_page, total_pages) = crate::calendar::calculate_pagination(start_idx, total_events, page_size);
        let page_text = (i18n.pagination_page_of)(current_page, total_pages);
        let (p_w, _) = Canvas::measure_text(&bold, &page_text, footer_sz);
        let center_x = (canvas.width as f32 - p_w) / 2.0;
        canvas.draw_text(&bold, &page_text, center_x as i32, text_y, footer_sz, COLOR_BLACK);

        // Indicador a la izquierda si hay eventos previos
        if start_idx > 0 {
            let prev_text = i18n.pagination_prev_hint;
            canvas.draw_text(&bold, prev_text, margin as i32, text_y, footer_sz, COLOR_DARK_GRAY);
        }

        // Indicador a la derecha si hay eventos futuros
        if start_idx + page_size < total_events {
            let next_text = i18n.pagination_next_hint;
            let (n_w, _) = Canvas::measure_text(&bold, next_text, footer_sz);
            let next_x = (canvas.width - margin) as f32 - n_w;
            canvas.draw_text(&bold, next_text, next_x as i32, text_y, footer_sz, COLOR_DARK_GRAY);
        }
    }
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

#[derive(Debug, Clone, Copy)]
struct CardLayout {
    card_h: u32,
    card_gap: u32,
    badge_h: u32,
    time_font_sz: f32,
    all_day_font_sz: f32,
    cal_font_sz: f32,
    summary_font_sz: f32,
    summary_two_line_sz: f32,
}

fn compute_section_headers_h(all_day_count: usize, timed_count: usize, scale: f32) -> u32 {
    let mut headers_h = 0;
    if all_day_count > 0 {
        headers_h += (42.0 * scale).round() as u32 + (18.0 * scale).round() as u32;
        if timed_count > 0 {
            headers_h += (22.0 * scale).round() as u32;
        }
    }
    if timed_count > 0 || (all_day_count + timed_count) == 0 {
        headers_h += (42.0 * scale).round() as u32 + (22.0 * scale).round() as u32;
    }
    headers_h
}

fn compute_card_layout(
    body_top_y: u32,
    footer_y: u32,
    all_day_count: usize,
    timed_count: usize,
    scale: f32,
) -> CardLayout {
    let visible_count = all_day_count + timed_count;
    let card_gap = match visible_count {
        0..=3 => (24.0 * scale).round() as u32,
        _ => (18.0 * scale).round() as u32,
    };

    let headers_h = compute_section_headers_h(all_day_count, timed_count, scale);
    let bottom_pad = (22.0 * scale).round() as u32;
    let usable_bottom = footer_y.saturating_sub(bottom_pad);
    let usable_body_h = usable_bottom.saturating_sub(body_top_y);
    let num_gaps = (all_day_count.saturating_sub(1) + timed_count.saturating_sub(1)) as u32;
    let cards_avail_h = usable_body_h.saturating_sub(headers_h + num_gaps * card_gap);

    let max_card_h = (280.0 * scale).round() as u32;
    let min_card_h = (120.0 * scale).round() as u32;
    let card_h = if visible_count == 0 {
        max_card_h
    } else {
        (cards_avail_h / visible_count as u32).clamp(min_card_h, max_card_h)
    };

    let summary_font_sz = if card_h >= (265.0 * scale).round() as u32 {
        (64.0 * scale).round()
    } else {
        (60.0 * scale).round()
    };

    CardLayout {
        card_h,
        card_gap,
        badge_h: (64.0 * scale).round() as u32,
        time_font_sz: (40.0 * scale).round(),
        all_day_font_sz: (36.0 * scale).round(),
        cal_font_sz: (32.0 * scale).round(),
        summary_font_sz,
        summary_two_line_sz: (46.0 * scale).round(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_event_card<F: Font>(
    canvas: &mut Canvas,
    card_x: u32,
    card_y: u32,
    card_w: u32,
    layout: &CardLayout,
    primary_badge: &str,
    primary_font_sz: f32,
    cal_name: &str,
    summary: &str,
    bold: &F,
    scale: f32,
) {
    canvas.draw_rounded_rect_fill(card_x, card_y, card_w, layout.card_h, 14, COLOR_LIGHT_GRAY);
    canvas.draw_rect_outline(card_x, card_y, card_w, layout.card_h, 2, COLOR_DARK_GRAY);

    let pad_x = (22.0 * scale).round() as u32;
    let badge_y = card_y + (22.0 * scale).round() as u32;
    let badge_pad_x = (17.0 * scale).round() as u32;
    let badge_gap = (14.0 * scale).round() as u32;
    let max_inner_w = card_w.saturating_sub(2 * pad_x);

    // Primary pill (time range or all-day label)
    let p_box_x = card_x + pad_x;
    let primary_text = fit_text_to_width(
        bold,
        primary_badge,
        primary_font_sz,
        (max_inner_w.saturating_sub(2 * badge_pad_x)) as f32,
    );
    let (p_w, _) = Canvas::measure_text(bold, &primary_text, primary_font_sz);
    let p_box_w = ((p_w + (2 * badge_pad_x) as f32).round() as u32).min(max_inner_w);
    canvas.draw_rounded_rect_fill(p_box_x, badge_y, p_box_w, layout.badge_h, 10, COLOR_BLACK);
    let p_text_y = badge_y as f32 + (layout.badge_h as f32 - primary_font_sz) / 2.0;
    canvas.draw_text(
        bold,
        &primary_text,
        (p_box_x + badge_pad_x) as i32,
        p_text_y as i32,
        primary_font_sz,
        COLOR_WHITE,
    );

    // Calendar name pill
    let cal_box_x = p_box_x + p_box_w + badge_gap;
    let right_limit = card_x + card_w - pad_x;
    let cal_pad_x = (14.0 * scale).round() as u32;
    let avail_cal_w = right_limit.saturating_sub(cal_box_x);
    if !cal_name.trim().is_empty() && avail_cal_w > 2 * cal_pad_x + (30.0 * scale).round() as u32 {
        let fitted_cal = fit_text_to_width(
            bold,
            cal_name,
            layout.cal_font_sz,
            (avail_cal_w - 2 * cal_pad_x) as f32,
        );
        if !fitted_cal.is_empty() {
            let (c_w, _) = Canvas::measure_text(bold, &fitted_cal, layout.cal_font_sz);
            let cal_box_w = ((c_w + (2 * cal_pad_x) as f32).round() as u32).min(avail_cal_w);
            canvas.draw_rounded_rect_fill(cal_box_x, badge_y, cal_box_w, layout.badge_h, 10, COLOR_DARK_GRAY);
            let cal_text_y = badge_y as f32 + (layout.badge_h as f32 - layout.cal_font_sz) / 2.0;
            canvas.draw_text(
                bold,
                &fitted_cal,
                (cal_box_x + cal_pad_x) as i32,
                cal_text_y as i32,
                layout.cal_font_sz,
                COLOR_WHITE,
            );
        }
    }

    // Event summary: large single-line headline when it fits, or wrapped onto 2 lines
    let clean_summary = summary.trim();
    let summary_x = card_x + pad_x;
    let max_summary_w = max_inner_w as f32;
    let below_y = badge_y + layout.badge_h;
    let below_h = (card_y + layout.card_h).saturating_sub(below_y);

    if Canvas::measure_text(bold, clean_summary, layout.summary_font_sz).0 <= max_summary_w {
        let line_h = layout.summary_font_sz.round() as u32;
        let summary_y = below_y
            + ((below_h.saturating_sub(line_h)) / 2)
                .saturating_sub((2.0 * scale).round() as u32);
        canvas.draw_text(
            bold,
            clean_summary,
            summary_x as i32,
            summary_y as i32,
            layout.summary_font_sz,
            COLOR_BLACK,
        );
    } else {
        let (line1, line2_opt) =
            wrap_text_two_lines(bold, clean_summary, layout.summary_two_line_sz, max_summary_w);
        let line_h = layout.summary_two_line_sz.round() as u32;
        if let Some(line2) = line2_opt {
            let line_gap = (8.0 * scale).round() as u32;
            let block_h = 2 * line_h + line_gap;
            let y1 = below_y + (below_h.saturating_sub(block_h)) / 2;
            let y2 = y1 + line_h + line_gap;
            canvas.draw_text(bold, &line1, summary_x as i32, y1 as i32, layout.summary_two_line_sz, COLOR_BLACK);
            canvas.draw_text(bold, &line2, summary_x as i32, y2 as i32, layout.summary_two_line_sz, COLOR_BLACK);
        } else {
            let y1 = below_y + (below_h.saturating_sub(line_h)) / 2;
            canvas.draw_text(bold, &line1, summary_x as i32, y1 as i32, layout.summary_two_line_sz, COLOR_BLACK);
        }
    }
}

fn fit_text_to_width<F: Font>(font: &F, text: &str, scale_px: f32, max_width: f32) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if Canvas::measure_text(font, text, scale_px).0 <= max_width {
        return text.to_string();
    }

    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let mut candidate: String = chars.iter().collect();
        candidate.push_str("...");
        if Canvas::measure_text(font, &candidate, scale_px).0 <= max_width {
            return candidate;
        }
    }
    String::new()
}

fn wrap_text_two_lines<F: Font>(
    font: &F,
    text: &str,
    scale_px: f32,
    max_width: f32,
) -> (String, Option<String>) {
    let trimmed = text.trim();
    if trimmed.is_empty() || max_width <= 0.0 {
        return (String::new(), None);
    }
    if Canvas::measure_text(font, trimmed, scale_px).0 <= max_width {
        return (trimmed.to_string(), None);
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() > 1 {
        let mut split_idx = 0;
        let mut line1 = String::new();
        for (i, word) in words.iter().enumerate() {
            let cand = if line1.is_empty() {
                (*word).to_string()
            } else {
                format!("{} {}", line1, word)
            };
            if Canvas::measure_text(font, &cand, scale_px).0 <= max_width {
                line1 = cand;
                split_idx = i + 1;
            } else {
                break;
            }
        }
        if split_idx > 0 && split_idx < words.len() {
            let rest = words[split_idx..].join(" ");
            let line2 = fit_text_to_width(font, &rest, scale_px, max_width);
            let l2 = if line2.is_empty() { None } else { Some(line2) };
            return (line1, l2);
        }
    }

    // Fallback: single overlong token — break at character boundary
    let chars: Vec<char> = trimmed.chars().collect();
    let mut break_at = 1.min(chars.len());
    for end in 1..=chars.len() {
        let cand: String = chars[..end].iter().collect();
        if Canvas::measure_text(font, &cand, scale_px).0 <= max_width {
            break_at = end;
        } else {
            break;
        }
    }
    let line1: String = chars[..break_at].iter().collect();
    let rest: String = chars[break_at..].iter().collect();
    if rest.trim().is_empty() {
        (line1, None)
    } else {
        let line2 = fit_text_to_width(font, rest.trim(), scale_px, max_width);
        let l2 = if line2.is_empty() { None } else { Some(line2) };
        (line1, l2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};


    #[test]
    fn test_render_dashboard_english_with_pagination() {
        let mut canvas = Canvas::new(1072, 1448);
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 8)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(19, 54, 0).unwrap())
            .and_local_timezone(offset)
            .unwrap();

        let events = vec![
            CalendarEvent::new_timed("Work", "Event 1", 9 * 60).with_end_minutes(10 * 60),
            CalendarEvent::new_timed("Work", "Event 2", 11 * 60).with_end_minutes(12 * 60),
            CalendarEvent::new_timed("Work", "Event 3", 13 * 60).with_end_minutes(14 * 60),
            CalendarEvent::new_timed("Work", "Event 4", 15 * 60).with_end_minutes(16 * 60),
            CalendarEvent::new_timed("Work", "Event 5", 17 * 60).with_end_minutes(18 * 60),
            CalendarEvent::new_timed("Work", "Event 6", 19 * 60).with_end_minutes(20 * 60),
        ];

        // Página 1 (eventos 0..4)
        render_dashboard(&mut canvas, &now, &events, 0, TimeFormat::H12, Some(95), Language::En);
        // Página 2 (eventos 2..6, últimos 4)
        render_dashboard(&mut canvas, &now, &events, 2, TimeFormat::H12, Some(95), Language::En);
        assert_eq!(canvas.width, 1072);
        assert_eq!(canvas.height, 1448);
    }

    #[test]
    fn test_card_layout_expands_cards_and_fonts_within_footer() {
        let scale = 1.0;
        let header_h = 210;
        let body_top_y = header_h + 28;
        let footer_y = 1448 - 65;

        // 4 timed events: use available height down close to footer with larger cards & fonts
        let layout4 = compute_card_layout(body_top_y, footer_y, 0, 4, scale);
        assert!(
            layout4.card_h >= 240,
            "expected 4-card height >= 240, got {}",
            layout4.card_h
        );
        assert!(
            layout4.time_font_sz >= 38.0,
            "expected time font >= 38px, got {}",
            layout4.time_font_sz
        );
        assert!(
            layout4.summary_font_sz >= 58.0,
            "expected summary font >= 58px, got {}",
            layout4.summary_font_sz
        );
        let bottom4 = body_top_y
            + compute_section_headers_h(0, 4, scale)
            + 4 * layout4.card_h
            + 3 * layout4.card_gap;
        assert!(bottom4 <= footer_y - 16, "4 cards overflowed footer: {} > {}", bottom4, footer_y - 16);
        assert!(
            footer_y - bottom4 <= 45,
            "too much unused space below 4 cards: {}px",
            footer_y - bottom4
        );

        // Mix: 1 all-day + 3 timed (4 total with both section headers) must also fit cleanly
        let layout_mix = compute_card_layout(body_top_y, footer_y, 1, 3, scale);
        assert!(
            layout_mix.card_h >= 230,
            "expected mixed 4-card height >= 230, got {}",
            layout_mix.card_h
        );
        let bottom_mix = body_top_y
            + compute_section_headers_h(1, 3, scale)
            + 4 * layout_mix.card_h
            + 2 * layout_mix.card_gap;
        assert!(
            bottom_mix <= footer_y - 16,
            "mixed 4 cards overflowed footer: {} > {}",
            bottom_mix,
            footer_y - 16
        );

        // 3 cards should also be larger than before
        let layout3 = compute_card_layout(body_top_y, footer_y, 0, 3, scale);
        assert!(layout3.card_h >= 260, "expected 3-card height >= 260, got {}", layout3.card_h);
    }

    #[test]
    fn test_wrap_text_two_lines_and_fit() {
        let bold = font_bold();
        let max_w = 500.0;
        let sz = 46.0;

        let (empty_l1, empty_l2) = wrap_text_two_lines(&bold, "   ", sz, max_w);
        assert_eq!(empty_l1, "");
        assert!(empty_l2.is_none());

        let (long_l1, long_l2) = wrap_text_two_lines(
            &bold,
            "Quarterly Architecture Review With Mobile and Embedded Teams",
            sz,
            max_w,
        );
        assert!(Canvas::measure_text(&bold, &long_l1, sz).0 <= max_w);
        let l2 = long_l2.expect("expected second wrapped line");
        assert!(Canvas::measure_text(&bold, &l2, sz).0 <= max_w);

        let (tok_l1, tok_l2) = wrap_text_two_lines(
            &bold,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            sz,
            max_w,
        );
        assert!(Canvas::measure_text(&bold, &tok_l1, sz).0 <= max_w);
        let tl2 = tok_l2.expect("expected token split across 2 lines");
        assert!(Canvas::measure_text(&bold, &tl2, sz).0 <= max_w);
    }

    #[test]
    fn test_render_dashboard_handles_mixed_all_day_and_long_titles() {
        let mut canvas = Canvas::new(1072, 1448);
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 8)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap())
            .and_local_timezone(offset)
            .unwrap();

        let events = vec![
            CalendarEvent::new_all_day("Very Long Calendar Name Here", "Annual Architecture Planning Day"),
            CalendarEvent::new_timed(
                "Engineering Team",
                "Weekly Deep Dive Into E-Ink Rendering and Recurrence Edge Cases Across Multiple Timezones",
                12 * 60,
            )
            .with_end_minutes(12 * 60 + 30),
            CalendarEvent::new_timed("Work", "Quick Sync", 14 * 60).with_end_minutes(15 * 60),
            CalendarEvent::new_timed("Personal", "Evening Gym Session", 18 * 60).with_end_minutes(19 * 60),
        ];

        render_dashboard(&mut canvas, &now, &events, 0, TimeFormat::H12, Some(88), Language::En);
        assert_eq!(canvas.width, 1072);
        assert_eq!(canvas.height, 1448);
    }
}
