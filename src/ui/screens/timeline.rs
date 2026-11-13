use crate::calendar::CalendarEvent;
use crate::config::{Language, TimeFormat};
use crate::ui::canvas::{Canvas, COLOR_BLACK, COLOR_DARK_GRAY, COLOR_LIGHT_GRAY, COLOR_WHITE};
use crate::ui::fonts::{font_bold, font_regular};
use crate::ui::i18n::get_translations;
use ab_glyph::Font;
use chrono::{DateTime, FixedOffset, Timelike};

/// Renderiza la vista de cronograma diario (Timeline View estilo Google Calendar)
pub fn render_timeline_dashboard(
    canvas: &mut Canvas,
    now: &DateTime<FixedOffset>,
    events: &[CalendarEvent],
    time_format: TimeFormat,
    battery_percent: Option<u32>,
    lang: Language,
) {
    canvas.fill(COLOR_WHITE);

    let bold = font_bold();
    let reg = font_regular();
    let i18n = get_translations(lang);

    let scale = (canvas.width as f32 / 1072.0).clamp(0.6, 1.4);
    let margin = (42.0 * scale).round() as u32;
    let content_w = canvas.width.saturating_sub(2 * margin);

    // =========================================================================
    // 1. ENCABEZADO SUPERIOR (Fecha, Reloj, Batería y Botón Salir — tamaños como en cards)
    // =========================================================================
    let header_h = (210.0 * scale).round() as u32;

    // Fila 1: Fecha completa (ancho completo superior sin colisionar con controles)
    let date_str = (i18n.format_date)(now);
    let date_sz = (42.0 * scale).round();
    canvas.draw_text(&bold, &date_str, margin as i32, (22.0 * scale) as i32, date_sz, COLOR_BLACK);

    // Fila 2: Reloj grande
    let time_str = format_current_time(now, time_format);
    let time_sz = (92.0 * scale).round();
    let clock_y = (84.0 * scale) as i32;
    canvas.draw_text(&bold, &time_str, margin as i32, clock_y, time_sz, COLOR_BLACK);

    // Botón [ SALIR ] / [ EXIT ] en esquina superior derecha
    let exit_w = (220.0 * scale).round() as u32;
    let exit_h = (88.0 * scale).round() as u32;
    let exit_x = canvas.width - margin - exit_w;
    let exit_y = (78.0 * scale) as u32;

    canvas.draw_rounded_rect_fill(exit_x, exit_y, exit_w, exit_h, 14, COLOR_BLACK);
    let exit_text = i18n.exit_button;
    let exit_sz = (36.0 * scale).round();
    let (e_w, _) = Canvas::measure_text(&bold, exit_text, exit_sz);
    let e_text_x = exit_x as f32 + (exit_w as f32 - e_w) / 2.0;
    let e_text_y = exit_y as f32 + (exit_h as f32 - exit_sz) / 2.0;
    canvas.draw_text(&bold, exit_text, e_text_x as i32, e_text_y as i32, exit_sz, COLOR_WHITE);

    // Indicador de Batería al lado izquierdo del botón [ SALIR ]
    if let Some(batt) = battery_percent {
        let batt_str = format!("{}%", batt);
        let batt_sz = (42.0 * scale).round();
        let (b_w, _) = Canvas::measure_text(&bold, &batt_str, batt_sz);
        let batt_w = (b_w + 36.0 * scale).round() as u32;
        let batt_h = (70.0 * scale).round() as u32;
        let batt_x = exit_x.saturating_sub(batt_w + (20.0 * scale) as u32);
        let batt_y = exit_y + (exit_h.saturating_sub(batt_h)) / 2;

        canvas.draw_rounded_rect_fill(batt_x, batt_y, batt_w, batt_h, 10, COLOR_LIGHT_GRAY);
        canvas.draw_rect_outline(batt_x, batt_y, batt_w, batt_h, 2, COLOR_DARK_GRAY);
        let b_text_x = batt_x as f32 + (batt_w as f32 - b_w) / 2.0;
        let b_text_y = batt_y as f32 + (batt_h as f32 - batt_sz) / 2.0;
        canvas.draw_text(&bold, &batt_str, b_text_x as i32, b_text_y as i32, batt_sz, COLOR_BLACK);
    }

    // Línea separadora del encabezado
    canvas.draw_line_h(margin, header_h, content_w, 2, COLOR_LIGHT_GRAY);

    let mut current_y = header_h + (14.0 * scale) as u32;

    // =========================================================================
    // 2. SECCIÓN DE EVENTOS "TODO EL DÍA"
    // =========================================================================
    let all_day_events: Vec<&CalendarEvent> = events.iter().filter(|e| e.is_all_day).collect();
    if !all_day_events.is_empty() {
        let chip_h = (74.0 * scale).round() as u32;
        let badge_sz = (26.0 * scale).round();
        let title_sz = (36.0 * scale).round();

        for ev in all_day_events.iter().take(2) {
            canvas.draw_rounded_rect_fill(margin, current_y, content_w, chip_h, 10, COLOR_LIGHT_GRAY);
            canvas.draw_rect_outline(margin, current_y, content_w, chip_h, 2, COLOR_DARK_GRAY);

            // Badge TODO EL DÍA con ancho dinámico
            let b_lbl = i18n.all_day_badge;
            let (lbl_w, _) = Canvas::measure_text(&bold, b_lbl, badge_sz);
            let badge_w = ((lbl_w + 30.0 * scale).round() as u32).max((160.0 * scale).round() as u32);
            let badge_pad = (6.0 * scale).round() as u32;
            let badge_box_h = chip_h.saturating_sub(badge_pad * 2);
            let badge_y = current_y + badge_pad;
            canvas.draw_rounded_rect_fill(margin + badge_pad, badge_y, badge_w, badge_box_h, 8, COLOR_DARK_GRAY);
            let lbl_x = (margin + badge_pad) as f32 + (badge_w as f32 - lbl_w) / 2.0;
            let lbl_y = badge_y as f32 + (badge_box_h as f32 - badge_sz) / 2.0;
            canvas.draw_text(&bold, b_lbl, lbl_x as i32, lbl_y as i32, badge_sz, COLOR_WHITE);

            // Título del evento ajustado al ancho disponible
            let text_x = margin + badge_pad + badge_w + (18.0 * scale) as u32;
            let max_title_w = (margin + content_w).saturating_sub(text_x + (14.0 * scale) as u32) as f32;
            let title_y = current_y as f32 + (chip_h as f32 - title_sz) / 2.0;
            let title = fit_text_to_width(&bold, &ev.summary, title_sz, max_title_w);
            canvas.draw_text(&bold, &title, text_x as i32, title_y as i32, title_sz, COLOR_BLACK);

            current_y += chip_h + (8.0 * scale) as u32;
        }

        canvas.draw_line_h(margin, current_y, content_w, 1, COLOR_LIGHT_GRAY);
        current_y += (10.0 * scale) as u32;
    }

    // =========================================================================
    // 3. CRONOGRAMA DIARIO (Eje vertical de horas + Bloques proporcionales)
    // =========================================================================
    let timed_events: Vec<&CalendarEvent> = events.iter().filter(|e| !e.is_all_day).collect();

    // Rango horario adaptativo: cubre desde las 06:00 hasta las 23:00 por defecto,
    // y se expande si hay eventos de madrugada o nocturnos.
    let mut start_hour: i32 = 6;
    let mut end_hour: i32 = 23;

    if let Some(min_start) = timed_events.iter().map(|e| e.start_minutes / 60).min() {
        start_hour = start_hour.min(min_start);
    }
    if let Some(max_end) = timed_events
        .iter()
        .map(|e| (e.end_minutes.unwrap_or(e.start_minutes + 60) + 59) / 60)
        .max()
    {
        end_hour = end_hour.max(max_end);
    }

    // Asegurar que la hora actual esté dentro del rango visible
    let now_hour = now.hour() as i32;
    start_hour = start_hour.min(now_hour).clamp(0, 23);
    end_hour = end_hour.max(now_hour).clamp(start_hour + 10, 24);

    let total_hours = (end_hour - start_hour + 1).max(12) as f32;
    let time_col_w = (122.0 * scale).round() as u32;

    let grid_top = current_y + (8.0 * scale) as u32;
    let grid_bottom = canvas.height.saturating_sub((28.0 * scale) as u32);
    let grid_height = grid_bottom.saturating_sub(grid_top);
    let px_per_hour = grid_height as f32 / total_hours;
    let px_per_minute = px_per_hour / 60.0;

    let event_area_x = margin + time_col_w;
    let event_area_w = content_w.saturating_sub(time_col_w);

    // Dibujar guías de horas y etiquetas temporales
    let hour_sz = (28.0 * scale).round();
    for h in start_hour..=end_hour {
        let hour_offset = (h - start_hour) as f32;
        let y_h = grid_top + (hour_offset * px_per_hour).round() as u32;

        if y_h <= grid_bottom {
            // Línea horizontal tenue en cada hora
            canvas.draw_line_h(event_area_x, y_h, event_area_w, 1, COLOR_LIGHT_GRAY);

            // Etiqueta horaria en la columna izquierda
            let h_label = format_hour_label(h, time_format);
            let (lbl_w, _) = Canvas::measure_text(&reg, &h_label, hour_sz);
            let lbl_x = (event_area_x as f32 - lbl_w - 12.0 * scale).max(margin as f32);
            let lbl_y = y_h as f32 - (hour_sz / 2.0);
            canvas.draw_text(&reg, &h_label, lbl_x as i32, lbl_y as i32, hour_sz, COLOR_DARK_GRAY);
        }
    }

    // Línea vertical que separa el eje horario de la zona de eventos
    canvas.draw_line_v(event_area_x, grid_top, grid_height, 2, COLOR_LIGHT_GRAY);

    // Asignar columnas a eventos concurrentes / superpuestos
    let event_layouts = layout_overlapping_events(&timed_events);

    let current_minutes = (now.hour() * 60 + now.minute()) as i32;

    // Renderizar bloques de eventos
    let title_font_sz = (32.0 * scale).round();
    let time_font_sz = (26.0 * scale).round();
    let min_block_h = (56.0 * scale).round() as u32;
    let two_line_min_h = (76.0 * scale).round() as u32;

    for layout in &event_layouts {
        let ev = layout.event;
        let start_m = ev.start_minutes;
        let end_m = ev.end_minutes.unwrap_or(ev.start_minutes + 60).max(start_m + 20);

        let start_offset_mins = (start_m - start_hour * 60).max(0) as f32;
        let end_offset_mins = (end_m - start_hour * 60).max(0) as f32;

        let b_top = grid_top + (start_offset_mins * px_per_minute).round() as u32;
        let b_bottom = grid_top + (end_offset_mins * px_per_minute).round() as u32;
        let b_h = b_bottom.saturating_sub(b_top).max(min_block_h);

        // Distribución horizontal entre columnas en caso de superposición
        let col_gap = (6.0 * scale).round() as u32;
        let total_cols = layout.total_cols.max(1);
        let col_w = (event_area_w.saturating_sub(col_gap * (total_cols as u32 - 1))) / total_cols as u32;
        let b_x = event_area_x + (layout.col as u32 * (col_w + col_gap)) + (6.0 * scale) as u32;
        let b_w = col_w.saturating_sub((10.0 * scale) as u32);

        let is_active = !ev.is_all_day && ev.start_minutes <= current_minutes && current_minutes < end_m;

        // Dibujar caja de evento
        canvas.draw_rounded_rect_fill(b_x, b_top, b_w, b_h, 8, COLOR_LIGHT_GRAY);
        let border_thickness = if is_active { 3 } else { 1 };
        let border_color = if is_active { COLOR_BLACK } else { COLOR_DARK_GRAY };
        canvas.draw_rect_outline(b_x, b_top, b_w, b_h, border_thickness, border_color);

        // Barra de acento vertical izquierda si el evento está en curso
        if is_active {
            canvas.draw_rect_fill(b_x, b_top, (8.0 * scale).round() as u32, b_h, COLOR_BLACK);
        }

        // Textos dentro del bloque de evento
        let left_inset = if is_active { (16.0 * scale) as u32 } else { (12.0 * scale) as u32 };
        let right_inset = (12.0 * scale) as u32;
        let text_pad_x = b_x + left_inset;
        let max_text_w = b_w.saturating_sub(left_inset + right_inset) as f32;

        if b_h >= two_line_min_h {
            // Dos filas: Título grande en negrita + Rango horario y calendario
            let mut text_cur_y = b_top + (8.0 * scale) as u32;
            let title_text = fit_text_to_width(&bold, &ev.summary, title_font_sz, max_text_w);
            canvas.draw_text(&bold, &title_text, text_pad_x as i32, text_cur_y as i32, title_font_sz, COLOR_BLACK);

            text_cur_y += title_font_sz as u32 + (4.0 * scale) as u32;

            let time_label = ev.time_label(time_format);
            let meta_full = if ev.calendar_name.trim().is_empty() {
                time_label
            } else {
                format!("{} • {}", time_label, ev.calendar_name)
            };
            let meta_text = fit_text_to_width(&reg, &meta_full, time_font_sz, max_text_w);
            canvas.draw_text(&reg, &meta_text, text_pad_x as i32, text_cur_y as i32, time_font_sz, COLOR_DARK_GRAY);
        } else {
            // Bloque compacto de una fila centrado verticalmente
            let time_label = ev.time_label(time_format);
            let inline_candidate = format!("{} · {}", ev.summary, time_label);
            let line_text = if Canvas::measure_text(&bold, &inline_candidate, title_font_sz).0 <= max_text_w {
                inline_candidate
            } else {
                fit_text_to_width(&bold, &ev.summary, title_font_sz, max_text_w)
            };
            let text_y = b_top as f32 + (b_h as f32 - title_font_sz) / 2.0;
            canvas.draw_text(&bold, &line_text, text_pad_x as i32, text_y as i32, title_font_sz, COLOR_BLACK);
        }
    }

    // =========================================================================
    // 4. LÍNEA INDICADORA DE LA HORA ACTUAL ("AHORA" / Red Line en E-Ink)
    // =========================================================================
    let now_offset_mins = current_minutes - start_hour * 60;
    if now_offset_mins >= 0 && now_offset_mins <= (total_hours * 60.0) as i32 {
        let y_now = grid_top + (now_offset_mins as f32 * px_per_minute).round() as u32;
        if y_now <= grid_bottom {
            // Punto indicador sobre el eje
            let dot_radius = (6.0 * scale).round() as i32;
            canvas.draw_circle_fill(event_area_x as i32, y_now as i32, dot_radius, COLOR_BLACK);

            // Línea horizontal que cruza el cronograma
            canvas.draw_line_h(event_area_x, y_now.saturating_sub(1), event_area_w, 3, COLOR_BLACK);
        }
    }

    // =========================================================================
    // 5. MENSAJE SI NO HAY EVENTOS EN TODO EL DÍA
    // =========================================================================
    if events.is_empty() {
        let empty_box_h = (150.0 * scale).round() as u32;
        let empty_box_y = grid_top + (grid_height.saturating_sub(empty_box_h) / 2);
        canvas.draw_rounded_rect_fill(margin + time_col_w + 30, empty_box_y, event_area_w.saturating_sub(60), empty_box_h, 12, COLOR_LIGHT_GRAY);
        canvas.draw_rect_outline(margin + time_col_w + 30, empty_box_y, event_area_w.saturating_sub(60), empty_box_h, 2, COLOR_DARK_GRAY);

        let no_title = i18n.no_events_title;
        let no_title_sz = (36.0 * scale).round();
        let (nt_w, _) = Canvas::measure_text(&bold, no_title, no_title_sz);
        let nt_x = (event_area_x as f32 + (event_area_w as f32 - nt_w) / 2.0).max(margin as f32);
        canvas.draw_text(&bold, no_title, nt_x as i32, (empty_box_y + (28.0 * scale) as u32) as i32, no_title_sz, COLOR_BLACK);

        let no_sub = i18n.no_events_subtitle;
        let no_sub_sz = (26.0 * scale).round();
        let (ns_w, _) = Canvas::measure_text(&reg, no_sub, no_sub_sz);
        let ns_x = (event_area_x as f32 + (event_area_w as f32 - ns_w) / 2.0).max(margin as f32);
        canvas.draw_text(&reg, no_sub, ns_x as i32, (empty_box_y + (84.0 * scale) as u32) as i32, no_sub_sz, COLOR_DARK_GRAY);
    }
}

// -----------------------------------------------------------------------------
// Estructuras y Algoritmo para Eventos Superpuestos (Column Allocation)
// -----------------------------------------------------------------------------

struct EventColumnLayout<'a> {
    event: &'a CalendarEvent,
    col: usize,
    total_cols: usize,
}

fn layout_overlapping_events<'a>(events: &[&'a CalendarEvent]) -> Vec<EventColumnLayout<'a>> {
    if events.is_empty() {
        return Vec::new();
    }

    // Ordenar por hora de inicio y luego por duración decreciente
    let mut sorted: Vec<&'a CalendarEvent> = events.to_vec();
    sorted.sort_by(|a, b| {
        let a_start = a.start_minutes;
        let b_start = b.start_minutes;
        if a_start != b_start {
            a_start.cmp(&b_start)
        } else {
            let a_dur = a.end_minutes.unwrap_or(a_start + 60) - a_start;
            let b_dur = b.end_minutes.unwrap_or(b_start + 60) - b_start;
            b_dur.cmp(&a_dur)
        }
    });

    // Agrupar en clusters de eventos que se tocan o solapan
    let mut clusters: Vec<Vec<&'a CalendarEvent>> = Vec::new();
    let mut current_cluster: Vec<&'a CalendarEvent> = Vec::new();
    let mut cluster_end_m = -1;

    for ev in sorted {
        let start_m = ev.start_minutes;
        let end_m = ev.end_minutes.unwrap_or(start_m + 60);

        if current_cluster.is_empty() || start_m < cluster_end_m {
            cluster_end_m = cluster_end_m.max(end_m);
            current_cluster.push(ev);
        } else {
            clusters.push(std::mem::take(&mut current_cluster));
            cluster_end_m = end_m;
            current_cluster.push(ev);
        }
    }
    if !current_cluster.is_empty() {
        clusters.push(current_cluster);
    }

    let mut result = Vec::new();

    // Asignar columnas dentro de cada cluster
    for cluster in clusters {
        let mut col_assignments: Vec<(&'a CalendarEvent, usize)> = Vec::new();
        let mut col_end_times: Vec<i32> = Vec::new();

        for ev in cluster {
            let start_m = ev.start_minutes;
            let end_m = ev.end_minutes.unwrap_or(start_m + 60);

            let mut placed_col = None;
            for (col_idx, last_end) in col_end_times.iter_mut().enumerate() {
                if start_m >= *last_end {
                    *last_end = end_m;
                    placed_col = Some(col_idx);
                    break;
                }
            }

            let col = match placed_col {
                Some(c) => c,
                None => {
                    col_end_times.push(end_m);
                    col_end_times.len() - 1
                }
            };

            col_assignments.push((ev, col));
        }

        let total_cols = col_end_times.len();
        for (ev, col) in col_assignments {
            result.push(EventColumnLayout {
                event: ev,
                col,
                total_cols,
            });
        }
    }

    result
}

fn format_current_time(now: &DateTime<FixedOffset>, format: TimeFormat) -> String {
    match format {
        TimeFormat::H12 => {
            let (is_pm, hour12) = now.hour12();
            let h = if hour12 == 0 { 12 } else { hour12 };
            format!("{:02}:{:02} {}", h, now.minute(), if is_pm { "PM" } else { "AM" })
        }
        TimeFormat::H24 => {
            format!("{:02}:{:02}", now.hour(), now.minute())
        }
    }
}

fn format_hour_label(hour: i32, format: TimeFormat) -> String {
    let h = hour.clamp(0, 24);
    match format {
        TimeFormat::H12 => {
            if h == 0 || h == 24 {
                "12 AM".to_string()
            } else if h < 12 {
                format!("{} AM", h)
            } else if h == 12 {
                "12 PM".to_string()
            } else {
                format!("{} PM", h - 12)
            }
        }
        TimeFormat::H24 => {
            format!("{:02}:00", h % 24)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_render_timeline_dashboard() {
        let mut canvas = Canvas::new(1072, 1448);
        let offset = FixedOffset::east_opt(-4 * 3600).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 8)
            .unwrap()
            .and_hms_opt(10, 15, 0)
            .unwrap()
            .and_local_timezone(offset)
            .unwrap();

        let events = vec![
            CalendarEvent::new_all_day("Work", "All-Day Event"),
            CalendarEvent::new_timed("Work", "Event 1", 9 * 60)
                .with_end_minutes(10 * 60),
            CalendarEvent::new_timed("Work", "Event 2", 10 * 60)
                .with_end_minutes(11 * 60),
            CalendarEvent::new_timed("Work", "Event 3", 10 * 60)
                .with_end_minutes(11 * 60 + 30),
            CalendarEvent::new_timed("Personal", "Event 4", 15 * 60)
                .with_end_minutes(16 * 60 + 30),
        ];

        render_timeline_dashboard(&mut canvas, &now, &events, TimeFormat::H24, Some(92), Language::En);
        assert_eq!(canvas.width, 1072);
        assert_eq!(canvas.height, 1448);
    }
}
